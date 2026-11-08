pub mod diagnostics;
mod store;
mod writer;

use logger::{LoggerRuntime, LoggerSettings, get_log_base_path, init_logger_subscriber_with_sink};

/// 本地文件提供者独立持有缓冲与存储，不依赖数据库或 ID 服务。
pub fn init_local_file_logger(settings: &LoggerSettings) -> Result<LoggerRuntime, String> {
    settings.validate_log_filter(logger::LoggerProvider::LocalFile)?;
    settings.local_file_buffer.validate()?;
    if settings.save_days != -1 && settings.save_days <= 0 {
        return Err("日志保留天数必须为正数或 -1".into());
    }
    let diagnostics = diagnostics::Diagnostics::new();
    let store = store::LocalFileStore::new(get_log_base_path(), diagnostics.clone());
    let writer = writer::LocalFileLogWriter::start(
        settings.local_file_buffer.clone(),
        store.clone(),
        diagnostics.clone(),
    );
    let filter_handle = init_logger_subscriber_with_sink(
        settings,
        writer.clone(),
        logger::LoggerProvider::LocalFile,
    )?;
    diagnostics.publish();
    let save_days = settings.save_days;
    let clear = tokio::spawn(async move {
        if save_days == -1 {
            return;
        }
        loop {
            if let Err(error) = store.clear_expired(save_days).await {
                diagnostics.failure(diagnostics::Operation::Cleanup, error);
            }
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
        }
    });
    LoggerRuntime::new_async(writer, None, Some(clear))
        .with_filter_handle(filter_handle)
        .with_filter_reload(settings)
}
