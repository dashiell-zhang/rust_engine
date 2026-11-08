# 组件配置

宿主通过 TOML 和环境变量选择数据库、文件、短信、邮件等组件。可以为不同环境复用同一份程序，并用外部配置提供连接和凭据。

## 配置加载

默认文件位于 `presentation/<宿主>/appsettings.toml`。加载顺序为基础文件、环境文件、`APP__` 环境变量，后者覆盖前者。

| 宿主 | 环境名来源，按优先级排列 |
| --- | --- |
| WebAPI | `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT`、`ASPNETCORE_ENVIRONMENT` |
| TaskService | `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT` |

未设置时使用 `Production`；开发 Compose 设置为 `Development`。环境文件名例如 `appsettings.Production.toml`。

`APP_CONFIG_PATH` 可选择另一份基础配置，值不带 `.toml` 后缀。例如：

```powershell
docker compose exec -T -e APP_CONFIG_PATH=/workspace/config/admin -e APP_ENVIRONMENT=Production dev cargo run --locked -p admin_webapi
```

上例要求事先准备 `config/admin.toml`，可另提供 `config/admin.Production.toml`。环境变量嵌套键使用双下划线，例如 `APP__DATABASE__CONNECTION`。

组件配置在启动时读取，修改后重启生效。日志过滤规则支持单独的[文件热更新](local-file-logger.md#配置文件自动更新)。

## 文件存储

本地文件配置示例：

```toml
[file]
component = "local"
root_path = "wwwroot"
public_base_url = "https://files.example.com/"
```

`component` 接受 `local`、`aliyun_oss` 和 `tencent_cos`，默认 `local`。切换云存储时填写对应的 `file.aliyun_oss` 或 `file.tencent_cos` 配置，完整字段参见[管理端配置](../presentation/admin_webapi/appsettings.toml)。

- 公开地址使用 `file.public_base_url`，与相对路径直接拼接；需要末尾斜杠时应在配置中写明。
- 云配置中的 `endpoint` 仅兼容读取，不启用自定义 API 地址；嵌套的 `url` 不替代 `public_base_url`。
- 管理端默认根目录为 `wwwroot` 并提供静态访问；客户端默认为工作目录，且不开放静态文件。
- 任务宿主默认文件根目录为 `presentation/task_service/storage`。邮件附件按宿主工作目录解析，不在文件根目录另行搜索。

环境覆盖示例：`APP__FILE__ALIYUN_OSS__USE_INTERNAL_ENDPOINT=true`。实际接口行为见[文件上传与下载](webapi-files.md)。

## 短信与邮件

任务服务通过 `sms.component` 选择 `aliyun`（默认）或 `tencent`，在 `sms.aliyun` 或 `sms.tencent` 中设置凭据和提供商参数。腾讯云组件在号码前拼接 `+86`，调用方传入国内号码。

SMTP 示例：

```toml
[message.smtp]
host = "smtp.example.com"
port = 587
username = "sender@example.com"
password = ""
```

密码可通过 `APP__MESSAGE__SMTP__PASSWORD` 提供。SMTP 使用 STARTTLS；发信参数和附件用法见[任务服务](task-service.md)。两个 WebAPI 不创建邮件发送组件。

## 为自定义组件读取配置

组件定义自己的可反序列化配置类型，由运行入口从 `Settings::extension::<T>()` 读取。例如：

```rust
#[derive(serde::Deserialize)]
struct ReportConfiguration {
    report: ReportSettings,
}

#[derive(serde::Deserialize)]
struct ReportSettings {
    endpoint: String,
}

let configuration = settings.extension::<ReportConfiguration>()?;
```

对应 TOML：

```toml
[report]
endpoint = "https://reports.example.com/"
```

`extension` 读取同一份合并后的启动配置。档案中的 `validate_configuration` 可在创建连接等资源前校验必需字段，资源工厂再按配置创建组件。宿主接入示例见[业务模块与服务装配](database-modules.md)。
