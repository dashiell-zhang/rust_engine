use axum::extract::multipart::Field;
use common::{
    app_error::AppError,
    data_helper::{DataModel, data_table_to_list_display_name, excel_to_data_table},
    io_helper::file_extension,
};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

mod form_file;
pub use form_file::{FormFile, require_multipart};

/// 将非空 XLSX 上传字段转换为模型列表，按属性名及显示名称绑定。
/// 临时文件位于可执行文件旁的 files 目录，解析完成或失败后自动清理。
pub async fn excel_to_list<T>(mut file: Field<'_>) -> Result<Vec<T>, AppError>
where
    T: DataModel + Default + Send + 'static,
{
    if !file
        .file_name()
        .is_some_and(|name| file_extension(name).eq_ignore_ascii_case(".xlsx"))
    {
        return Err(invalid_file());
    }

    // Field 不提供可靠的文件长度，先读到非空块再创建临时文件。
    let first_chunk = loop {
        let Some(chunk) = file.chunk().await.map_err(read_error)? else {
            return Err(invalid_file());
        };
        if !chunk.is_empty() {
            break chunk;
        }
    };

    // 在同一个阻塞任务中创建文件及其清理句柄。等待期间请求取消时，
    // 任务返回值会被释放，不能留下“文件已创建但守卫尚未建立”的窗口。
    let temporary = tokio::task::spawn_blocking(create_temporary)
        .await
        .map_err(|error| AppError::internal(format!("创建 Excel 临时文件任务失败: {error}")))?
        .map_err(|error| AppError::internal(format!("创建 Excel 临时文件失败: {error}")))?;
    let (handle, path) = temporary.into_parts();
    let mut writer = tokio::fs::File::from_std(handle);
    writer.write_all(&first_chunk).await.map_err(write_error)?;
    while let Some(chunk) = file.chunk().await.map_err(read_error)? {
        writer.write_all(&chunk).await.map_err(write_error)?;
    }
    writer.flush().await.map_err(write_error)?;
    drop(writer);

    // 同步解析留在阻塞池；清理句柄跟随工作任务，不能在读取中途删除路径。
    tokio::task::spawn_blocking(move || {
        let cleanup = path;
        let file_path = cleanup.to_str().ok_or_else(parse_error)?;
        let table = excel_to_data_table(file_path, true)
            .map_err(|_| parse_error())?
            .ok_or_else(parse_error)?;
        data_table_to_list_display_name::<T>(&table).map_err(|_| parse_error())
    })
    .await
    .map_err(|error| AppError::internal(format!("解析 Excel 上传任务失败: {error}")))?
}

fn create_temporary() -> std::io::Result<NamedTempFile> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| std::io::Error::other("无法定位宿主可执行文件目录"))?
        .join("files");
    std::fs::create_dir_all(&directory)?;
    let name = uuid::Uuid::new_v4().to_string();
    tempfile::Builder::new()
        .prefix(&name)
        .suffix(".xlsx")
        .rand_bytes(0)
        .tempfile_in(directory)
}

fn invalid_file() -> AppError {
    AppError::bad_request("文件格式不正确或文件内容为空")
}

fn parse_error() -> AppError {
    AppError::bad_request("文件内容解析失败")
}

fn read_error(error: axum::extract::multipart::MultipartError) -> AppError {
    AppError::internal(format!("读取 Excel 上传内容失败: {error}"))
}

fn write_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("写入 Excel 临时文件失败: {error}"))
}
