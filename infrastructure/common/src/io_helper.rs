use std::{
    fs::{self, File},
    io::{self},
    path::{Path, PathBuf},
};

use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// 临时文件清理守卫
///
/// 在作用域提前退出或异步 Future 被取消时同步尽力删除临时文件
pub struct TemporaryFileGuard {
    /// 临时文件路径
    path: PathBuf,

    /// 退出作用域时是否需要清理
    cleanup: bool,
}

impl TemporaryFileGuard {
    /// 创建临时文件清理守卫
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            cleanup: true,
        }
    }

    /// 获取临时文件路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 保留临时文件并停止自动清理
    pub fn preserve(&mut self) {
        self.cleanup = false;
    }
}

impl Drop for TemporaryFileGuard {
    /// 作用域退出时尽力清理临时文件
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 删除指定文件
pub fn delete_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();

    let Ok(metadata) = fs::symlink_metadata(path) else {
        return true;
    };

    if metadata.file_type().is_symlink() {
        return fs::remove_file(path).is_ok();
    }

    if clear_readonly_recursively(path).is_err() {
        return false;
    }

    if metadata.is_dir() {
        fs::remove_dir_all(path).is_ok()
    } else {
        fs::remove_file(path).is_ok()
    }
}

/// 获取文件大小的友好显示文本
pub fn file_length_to_string(file_length: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let length = file_length as f64;

    if length < KB {
        format!("{length:.2} Byte")
    } else if length < MB {
        format!("{:.2} K", length / KB)
    } else if length < GB {
        format!("{:.2} M", length / MB)
    } else if length < TB {
        format!("{:.2} G", length / GB)
    } else {
        format!("{:.2} T", length / TB)
    }
}

/// 获取目录下的全部文件
pub fn get_folder_all_files(
    folder_path: impl AsRef<Path>,
    include_subfolders: bool,
) -> io::Result<Vec<PathBuf>> {
    let folder_path = folder_path.as_ref();
    let mut files = Vec::new();

    if include_subfolders {
        for entry in WalkDir::new(folder_path) {
            let entry = entry.map_err(io::Error::other)?;
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }
    } else {
        for entry in fs::read_dir(folder_path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                files.push(entry.path());
            }
        }
    }

    Ok(files)
}

/// 将指定文件压缩为 Zip 文件
pub fn compress_file_zip(
    file_path: impl AsRef<Path>,
    zip_path: impl AsRef<Path>,
) -> io::Result<()> {
    let file_path = file_path.as_ref();
    let zip_path = zip_path.as_ref();
    ensure_file_zip_output_is_distinct(file_path, zip_path)?;
    let file_name = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件名无效"))?;

    create_zip_atomically(zip_path, |zip| {
        zip.start_file(file_name, default_zip_options())?;
        let mut input = File::open(file_path)?;
        io::copy(&mut input, zip)?;
        Ok(())
    })
}

/// 将指定目录压缩为 Zip 文件
pub fn compress_directory_zip(
    folder_path: impl AsRef<Path>,
    zip_path: impl AsRef<Path>,
) -> io::Result<()> {
    let folder_path = folder_path.as_ref();
    let zip_path = zip_path.as_ref();
    ensure_zip_not_inside_source(folder_path, zip_path)?;

    create_zip_atomically(zip_path, |zip| {
        let options = default_zip_options();

        for entry in WalkDir::new(folder_path) {
            let entry = entry.map_err(io::Error::other)?;
            let path = entry.path();
            let relative = path.strip_prefix(folder_path).map_err(io::Error::other)?;

            if relative.as_os_str().is_empty() {
                continue;
            }

            let name = relative.to_string_lossy().replace('\\', "/");

            if entry.file_type().is_dir() {
                zip.add_directory(format!("{name}/"), options)?;
                continue;
            }

            if entry.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("不支持压缩符号链接: {}", path.display()),
                ));
            }

            zip.start_file(name, options)?;
            let mut input = File::open(path)?;
            io::copy(&mut input, zip)?;
        }

        Ok(())
    })
}

/// 解压 Zip 文件到指定目录
pub fn decompress_zip(zip_path: impl AsRef<Path>, folder_path: impl AsRef<Path>) -> io::Result<()> {
    let zip_path = zip_path.as_ref();
    let folder_path = folder_path.as_ref();
    fs::create_dir_all(folder_path)?;
    let folder_path = fs::canonicalize(folder_path)?;

    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Zip 文件包含不允许的符号链接: {}", entry.name()),
            ));
        }

        let enclosed_name = entry.enclosed_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Zip 文件路径无效: {}", entry.name()),
            )
        })?;

        if entry.is_dir() {
            let output_path = prepare_zip_output_directory(&folder_path, &enclosed_name)?;
            fs::create_dir_all(&output_path)?;
            continue;
        }

        let output_path = prepare_zip_output_file(&folder_path, &enclosed_name)?;

        let mut output = File::create(&output_path)?;
        io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

/// 准备 Zip 解压目录路径，拒绝目标路径中已有的符号链接
fn prepare_zip_output_directory(base_path: &Path, relative_path: &Path) -> io::Result<PathBuf> {
    let output_path = prepare_zip_output_parent(base_path, relative_path)?;
    reject_existing_symlink(&output_path)?;
    Ok(output_path)
}

/// 准备 Zip 解压文件路径，拒绝父目录和目标文件上的符号链接
fn prepare_zip_output_file(base_path: &Path, relative_path: &Path) -> io::Result<PathBuf> {
    let output_path = prepare_zip_output_parent(base_path, relative_path)?;
    reject_existing_symlink(&output_path)?;
    Ok(output_path)
}

/// 创建解压目标父目录，并确保沿途不经过符号链接
fn prepare_zip_output_parent(base_path: &Path, relative_path: &Path) -> io::Result<PathBuf> {
    let output_path = base_path.join(relative_path);
    let parent = output_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Zip 输出路径无效"))?;
    ensure_directory_without_symlink(base_path, parent)?;
    Ok(output_path)
}

/// 确保目录存在且不是符号链接
fn ensure_directory_without_symlink(base_path: &Path, directory: &Path) -> io::Result<()> {
    let relative = directory
        .strip_prefix(base_path)
        .map_err(io::Error::other)?;
    let mut current = base_path.to_path_buf();

    reject_existing_symlink(&current)?;
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Zip 解压路径包含符号链接: {}", current.display()),
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("Zip 解压路径不是目录: {}", current.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

/// 拒绝已存在的符号链接路径
fn reject_existing_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("路径不能是符号链接: {}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// 获取默认 Zip 选项
fn default_zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// 确保目录压缩输出文件不在源目录内部
fn ensure_zip_not_inside_source(folder_path: &Path, zip_path: &Path) -> io::Result<()> {
    let source = fs::canonicalize(folder_path)?;
    let zip_full_path = normalize_zip_output_path(zip_path)?;
    let zip_identity = canonicalize_existing_or_normalized(&zip_full_path)?;

    if zip_identity.starts_with(&source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Zip 输出文件不能位于被压缩目录内部",
        ));
    }

    Ok(())
}

/// 确保单文件压缩的输入文件与输出 Zip 不为同一路径
fn ensure_file_zip_output_is_distinct(file_path: &Path, zip_path: &Path) -> io::Result<()> {
    let source = fs::canonicalize(file_path)?;
    let zip_full_path = normalize_zip_output_path(zip_path)?;
    let zip_identity = canonicalize_existing_or_normalized(&zip_full_path)?;

    if source == zip_identity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Zip 输出文件不能与被压缩文件相同",
        ));
    }

    Ok(())
}

/// 通过同目录临时文件创建并原子替换 Zip 输出
fn create_zip_atomically(
    zip_path: &Path,
    write_entries: impl FnOnce(&mut ZipWriter<File>) -> io::Result<()>,
) -> io::Result<()> {
    let output_path = normalize_zip_output_path(zip_path)?;
    reject_existing_symlink(&output_path)?;

    let temp_path = temporary_sibling_path(&output_path, "tmp");
    let mut temp_guard = TemporaryFileGuard::new(temp_path);
    let zip_file = File::create(temp_guard.path())?;
    let mut zip = ZipWriter::new(zip_file);

    write_entries(&mut zip)?;
    let zip_file = zip.finish()?;
    zip_file.sync_all()?;
    drop(zip_file);

    replace_file(temp_guard.path(), &output_path)?;
    temp_guard.preserve();
    Ok(())
}

/// 规范化可能尚不存在的 Zip 输出路径
fn normalize_zip_output_path(zip_path: &Path) -> io::Result<PathBuf> {
    let zip_name = zip_path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Zip 文件名无效"))?;
    let parent = zip_path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent)?;
    Ok(fs::canonicalize(parent)?.join(zip_name))
}

/// 获取已存在路径的规范路径，否则返回已规范化的新路径
fn canonicalize_existing_or_normalized(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

/// 生成与目标文件同目录的唯一临时路径
fn temporary_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "archive.zip".into());
    file_name.push(format!(".{}.{}", uuid::Uuid::now_v7(), suffix));
    path.with_file_name(file_name)
}

/// 使用临时文件替换目标文件
pub(crate) fn replace_file(temp_path: &Path, output_path: &Path) -> io::Result<()> {
    match fs::rename(temp_path, output_path) {
        Ok(()) => Ok(()),
        Err(_) if output_path.exists() => replace_existing_file(temp_path, output_path),
        Err(error) => Err(error),
    }
}

/// 替换已存在的目标文件并在失败时尽量恢复
fn replace_existing_file(temp_path: &Path, output_path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(output_path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "输出路径已存在且不是普通文件",
        ));
    }

    let backup_path = temporary_sibling_path(output_path, "bak");
    fs::rename(output_path, &backup_path)?;
    let mut backup_guard = TemporaryFileGuard::new(backup_path);

    match fs::rename(temp_path, output_path) {
        Ok(()) => Ok(()),
        Err(error) => match fs::rename(backup_guard.path(), output_path) {
            Ok(()) => Err(error),
            Err(restore_error) => {
                backup_guard.preserve();
                Err(io::Error::other(format!(
                    "替换文件失败: {error}; 恢复原文件失败: {restore_error}; 原文件保留在 {}",
                    backup_guard.path().display()
                )))
            }
        },
    }
}

/// 递归清理只读属性
fn clear_readonly_recursively(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        for entry in WalkDir::new(path).contents_first(true) {
            let entry = entry.map_err(io::Error::other)?;
            clear_single_readonly(entry.path())?;
        }
        clear_single_readonly(path)?;
    } else {
        clear_single_readonly(path)?;
    }

    Ok(())
}

/// 清理单个路径的只读属性
fn clear_single_readonly(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }

    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }

    Ok(())
}
