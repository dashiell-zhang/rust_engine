use std::{
    fs::{self, File},
    io::{self},
    path::{Path, PathBuf},
};

use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// 删除指定文件
pub fn delete_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();

    if !path.exists() {
        return true;
    }

    if clear_readonly_recursively(path).is_err() {
        return false;
    }

    if path.is_dir() {
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
    let file_name = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件名无效"))?;

    let zip_file = ensure_zip_parent_and_open(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);
    let options = default_zip_options();

    zip.start_file(file_name, options)?;
    let mut input = File::open(file_path)?;
    io::copy(&mut input, &mut zip)?;
    zip.finish()?;

    Ok(())
}

/// 将指定目录压缩为 Zip 文件
pub fn compress_directory_zip(
    folder_path: impl AsRef<Path>,
    zip_path: impl AsRef<Path>,
) -> io::Result<()> {
    let folder_path = folder_path.as_ref();
    let zip_path = zip_path.as_ref();
    let zip_file = ensure_zip_parent_and_open(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);
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

        zip.start_file(name, options)?;
        let mut input = File::open(path)?;
        io::copy(&mut input, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}

/// 解压 Zip 文件到指定目录
pub fn decompress_zip(zip_path: impl AsRef<Path>, folder_path: impl AsRef<Path>) -> io::Result<()> {
    let zip_path = zip_path.as_ref();
    let folder_path = folder_path.as_ref();
    fs::create_dir_all(folder_path)?;

    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(folder_path)?;

    Ok(())
}

/// 创建 Zip 输出文件并确保父目录存在
fn ensure_zip_parent_and_open(zip_path: &Path) -> io::Result<File> {
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)?;
    }

    File::create(zip_path)
}

/// 获取默认 Zip 选项
fn default_zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
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
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }

    Ok(())
}
