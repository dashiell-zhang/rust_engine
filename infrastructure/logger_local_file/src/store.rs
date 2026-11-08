use crate::diagnostics::{Diagnostics, Operation};
use chrono::{DateTime, NaiveDate, Utc};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
    sync::{Mutex, MutexGuard, OnceCell},
};

#[derive(Default)]
struct StoreState {
    pending: Option<(PathBuf, u64)>,
    verified: Option<(PathBuf, u64)>,
}

/// 追加、尾部恢复与清理使用同一把锁；未确认的小时段始终保留回退位置。
pub(crate) struct LocalFileStore {
    base_path: PathBuf,
    state: Mutex<StoreState>,
    recovered: OnceCell<()>,
    diagnostics: Arc<Diagnostics>,
}

pub(crate) struct AppendSession<'a> {
    pub(crate) writer: BufWriter<File>,
    state: MutexGuard<'a, StoreState>,
    path: PathBuf,
}

impl AppendSession<'_> {
    pub(crate) async fn finish(mut self) -> io::Result<()> {
        self.writer.flush().await?;
        let file = self.writer.into_inner();
        let length = file.metadata().await?.len();
        // 等待 Tokio 已发出的文件操作完成后关闭，再允许清理或下一个小时段。
        drop(file.into_std().await);
        self.state.pending = None;
        self.state.verified = Some((self.path, length));
        Ok(())
    }
}

impl LocalFileStore {
    pub(crate) fn new(base_path: PathBuf, diagnostics: Arc<Diagnostics>) -> Arc<Self> {
        Arc::new(Self {
            base_path,
            state: Mutex::new(StoreState::default()),
            recovered: OnceCell::new(),
            diagnostics,
        })
    }

    pub(crate) async fn begin_append(
        &self,
        time: DateTime<Utc>,
        capacity: usize,
    ) -> io::Result<AppendSession<'_>> {
        let path = self
            .base_path
            .join(format!("{}.log", time.format("%Y%m%d%H")));
        let mut state = self.state.lock().await;
        if state
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| pending != &path)
        {
            return Err(io::Error::other("必须先完成等待重试的小时文件"));
        }
        fs::create_dir_all(&self.base_path).await?;
        let mut file = open_locked(&path, state.pending.is_none()).await?;
        let mut length = file.metadata().await?.len();
        if state.pending.is_none() {
            if state.verified.as_ref() != Some(&(path.clone(), length)) {
                self.repair_tail(&mut file, &path).await?;
                length = file.metadata().await?.len();
            }
            state.pending = Some((path.clone(), length));
        }
        let offset = state.pending.as_ref().expect("已记录重试位置").1;
        if length < offset {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "等待重试的文件已被外部截短",
            ));
        }
        if length != offset {
            file.set_len(offset).await?;
        }
        file.seek(SeekFrom::Start(offset)).await?;
        Ok(AppendSession {
            writer: BufWriter::with_capacity(capacity, file),
            state,
            path,
        })
    }

    pub(crate) async fn recover(&self) {
        self.recovered
            .get_or_init(|| async {
                if let Err(error) = self.recover_inner().await {
                    self.diagnostics.failure(Operation::Recovery, error);
                }
            })
            .await;
    }

    async fn recover_inner(&self) -> io::Result<()> {
        let state = self.state.lock().await;
        let mut entries = match fs::read_dir(&self.base_path).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !is_hourly_file(&path)
                || !entry.file_type().await?.is_file()
                || state
                    .pending
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == &path)
            {
                continue;
            }
            let result = async {
                let mut file = open_locked(&path, false).await?;
                self.repair_tail(&mut file, &path).await
            }
            .await;
            if let Err(error) = result {
                self.diagnostics.failure(Operation::Recovery, error);
            }
        }
        Ok(())
    }

    async fn repair_tail(&self, file: &mut File, path: &Path) -> io::Result<()> {
        let length = file.metadata().await?.len();
        if length == 0 {
            return Ok(());
        }
        let mut buffer = [0u8; 4096];
        let mut end = length;
        let mut valid_length = 0;
        while end > 0 {
            let count = end.min(buffer.len() as u64) as usize;
            file.seek(SeekFrom::Start(end - count as u64)).await?;
            file.read_exact(&mut buffer[..count]).await?;
            if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
                valid_length = end - count as u64 + index as u64 + 1;
                break;
            }
            end -= count as u64;
        }
        if valid_length == length {
            return Ok(());
        }
        let rejected_path = path.with_file_name(format!(
            "{}.tail-{}.rejected",
            path.file_name().expect("小时文件名").to_string_lossy(),
            uuid::Uuid::new_v4().simple()
        ));
        let rejected = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(rejected_path)
            .await?;
        let mut rejected = BufWriter::with_capacity(65536, rejected);
        file.seek(SeekFrom::Start(valid_length)).await?;
        tokio::io::copy(&mut (&mut *file).take(length - valid_length), &mut rejected).await?;
        rejected.flush().await?;
        drop(rejected.into_inner().into_std().await);
        // 原始尾部成功保存并关闭后才截短；保存失败或取消时保持原文件。
        file.set_len(valid_length).await?;
        file.flush().await?;
        self.diagnostics
            .failure(Operation::Recovery, "已保留未完成的日志尾部");
        Ok(())
    }

    pub(crate) async fn clear_expired(&self, save_days: i64) -> io::Result<()> {
        if save_days == -1 {
            return Ok(());
        }
        let state = self.state.lock().await;
        let cutoff = chrono::Duration::try_days(save_days)
            .and_then(|duration| Utc::now().checked_sub_signed(duration))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let mut entries = match fs::read_dir(&self.base_path).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !is_hourly_file(&path)
                || !entry.file_type().await?.is_file()
                || state
                    .pending
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == &path)
            {
                continue;
            }
            let result = async {
                // 清理只依赖目录删除权限，不要求日志文件可写或额外取得文件锁。
                // 本写入器的追加与待重试文件仍由上方状态锁和 pending 检查保护。
                let modified: DateTime<Utc> = fs::metadata(&path).await?.modified()?.into();
                if modified < cutoff {
                    fs::remove_file(&path).await?;
                }
                Ok::<_, io::Error>(())
            }
            .await;
            if let Err(error) = result
                && error.kind() != io::ErrorKind::NotFound
            {
                self.diagnostics.failure(Operation::Cleanup, error);
            }
        }
        Ok(())
    }
}

async fn open_locked(path: &Path, create: bool) -> io::Result<File> {
    let path = path.to_owned();
    let file = tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .open(path)?;
        file.try_lock().map_err(io::Error::other)?;
        Ok::<_, io::Error>(file)
    })
    .await??;
    Ok(File::from_std(file))
}

fn is_hourly_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let Some(hour) = name.strip_suffix(".log") else {
        return false;
    };
    hour.len() == 10
        && hour.bytes().all(|byte| byte.is_ascii_digit())
        && &hour[..4] != "0000"
        && NaiveDate::parse_from_str(&hour[..8], "%Y%m%d").is_ok()
        && hour[8..].parse::<u32>().is_ok_and(|hour| hour < 24)
}
