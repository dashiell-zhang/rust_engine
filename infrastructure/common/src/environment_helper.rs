use std::{env, path::Path};

/// 根据命令行参数决定是否切换工作目录到应用基目录
pub fn change_directory(args: &[String], base_directory: &Path) -> Result<bool, std::io::Error> {
    if !should_change_directory(args) {
        return Ok(false);
    }

    env::set_current_dir(base_directory)?;
    Ok(true)
}

/// 判断命令行参数中是否要求切换工作目录
pub fn should_change_directory(args: &[String]) -> bool {
    args.iter().any(|item| {
        let normalized = item.trim().to_ascii_lowercase();
        matches!(normalized.as_str(), "--cd" | "/cd" | "cd")
            || matches!(normalized.as_str(), "--cd=true" | "/cd=true" | "cd=true")
    })
}
