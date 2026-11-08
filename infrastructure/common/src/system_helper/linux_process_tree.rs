//! Linux 按父子关系终止进程树，不依赖后代保持同一个进程组或会话。

use std::{
    fmt, fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

/// 进程开始时间用于拒绝枚举后已被复用的 PID。
#[derive(Clone, Copy, PartialEq, Eq)]
struct ProcessIdentity {
    pid: libc::pid_t,
    started: u64,
}

struct ProcessInfo {
    identity: ProcessIdentity,
    parent: libc::pid_t,
}

/// 支持 pidfd 时通过内核句柄发信号，避免检查后 PID 被复用的竞态。
struct ProcessHandle {
    identity: ProcessIdentity,
    descriptor: Option<OwnedFd>,
}

impl ProcessHandle {
    fn open(identity: ProcessIdentity) -> io::Result<Option<Self>> {
        // SAFETY: pidfd_open 只接收正数 PID 和零标志，不传递指针。
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, identity.pid, 0u32) };
        let descriptor = if descriptor >= 0 {
            // SAFETY: 成功的 pidfd_open 返回一个新建的独占文件描述符。
            Some(unsafe { OwnedFd::from_raw_fd(descriptor as i32) })
        } else {
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::ESRCH) => return Ok(None),
                // 旧内核沿用 PID 信号接口，但每次发信号前核对开始时间。
                Some(libc::ENOSYS) => None,
                _ => return Err(error),
            }
        };

        if !read_process(identity.pid)?.is_some_and(|value| value.identity == identity) {
            return Ok(None);
        }
        Ok(Some(Self {
            identity,
            descriptor,
        }))
    }

    /// 返回 false 表示进程已退出或其 PID 已被复用。
    fn signal(&self, signal: libc::c_int) -> io::Result<bool> {
        let result = if let Some(descriptor) = &self.descriptor {
            // SAFETY: descriptor 由本对象持有；空 siginfo 和零标志等价于普通进程信号。
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    descriptor.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    0u32,
                )
            }
        } else {
            if !read_process(self.identity.pid)?
                .is_some_and(|value| value.identity == self.identity)
            {
                return Ok(false);
            }
            // SAFETY: 正数 PID 表示单个已核对身份的进程，不会向进程组发送信号。
            unsafe { libc::kill(self.identity.pid, signal) as libc::c_long }
        };
        if result == 0 {
            Ok(true)
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                Ok(false)
            } else {
                Err(error)
            }
        }
    }
}

/// 暂停、枚举、终止当前进程，再处理已记录的直接子进程。
pub(super) fn terminate(process_id: u32) -> io::Result<()> {
    let pid = libc::pid_t::try_from(process_id)
        .ok()
        .filter(|value| *value > 0 && process_id != std::process::id())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Shell 进程 ID 无效"))?;
    let Some(root) = read_process(pid)? else {
        return Ok(());
    };
    let mut pending = vec![root.identity];
    let mut failures = Vec::new();
    while let Some(identity) = pending.pop() {
        let process = match ProcessHandle::open(identity) {
            Ok(Some(process)) => process,
            Ok(None) => continue,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        match process.signal(libc::SIGSTOP) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                failures.push(error);
                continue;
            }
        }

        // 枚举失败仍终止已暂停的进程，不能将其留在停止状态。
        let children = match children_of(identity.pid) {
            Ok(children) => children,
            Err(error) => {
                failures.push(error);
                Vec::new()
            }
        };
        if let Err(error) = process.signal(libc::SIGKILL) {
            failures.push(error);
        }
        pending.extend(children.into_iter().rev());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(ProcessTreeErrors(failures)))
    }
}

/// /proc 顶层包含进程；父 PID 与进程组、会话 ID 相互独立。
fn children_of(parent: libc::pid_t) -> io::Result<Vec<ProcessIdentity>> {
    let mut children = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        if pid <= 0 || pid == parent || u32::try_from(pid).ok() == Some(std::process::id()) {
            continue;
        }
        // 与源枚举相同，跳过已消失或无法读取父进程信息的单个候选进程。
        if let Ok(Some(process)) = read_process(pid)
            && process.parent == parent
        {
            children.push(process.identity);
        }
    }
    Ok(children)
}

fn read_process(pid: libc::pid_t) -> io::Result<Option<ProcessInfo>> {
    let bytes = match fs::read(format!("/proc/{pid}/stat")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    // comm 允许空格、换行和右括号，不能按整行空格或第一个右括号拆分。
    let name_end = bytes
        .iter()
        .rposition(|byte| *byte == b')')
        .ok_or_else(invalid_stat)?;
    let mut fields = bytes[name_end + 1..]
        .split(u8::is_ascii_whitespace)
        .filter(|field| !field.is_empty());
    let parent = parse_field::<libc::pid_t>(fields.nth(1))?; // 字段 4：ppid
    let started = parse_field::<u64>(fields.nth(17))?; // 字段 22：starttime
    Ok(Some(ProcessInfo {
        identity: ProcessIdentity { pid, started },
        parent,
    }))
}

fn parse_field<T: std::str::FromStr>(field: Option<&[u8]>) -> io::Result<T> {
    field
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse().ok())
        .ok_or_else(invalid_stat)
}

fn invalid_stat() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Linux 进程状态格式无效")
}

#[derive(Debug)]
struct ProcessTreeErrors(Vec<io::Error>);

impl fmt::Display for ProcessTreeErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Shell 进程树清理失败（{} 项）", self.0.len())?;
        for error in &self.0 {
            write!(formatter, "；{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProcessTreeErrors {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.first().map(|error| error as _)
    }
}
