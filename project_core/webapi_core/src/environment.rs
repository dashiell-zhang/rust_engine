use std::sync::OnceLock;

/// 进程启动环境；Rust 显式配置优先，其次沿用 WebApplication 的环境变量顺序。
pub fn name() -> &'static str {
    static ENVIRONMENT: OnceLock<String> = OnceLock::new();
    ENVIRONMENT.get_or_init(|| {
        [
            "APP_ENVIRONMENT",
            "DOTNET_ENVIRONMENT",
            "ASPNETCORE_ENVIRONMENT",
        ]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "Production".to_string())
    })
}

/// 环境判断与编译配置独立；Release 也可运行在 Development 环境。
pub fn is_development() -> bool {
    name().eq_ignore_ascii_case("Development")
}
