# 对外 HTTP 调用

`http_client::HttpClientFactory` 统一管理出站 HTTP 连接，适合调用第三方 API、下载文件和使用带签名的命名客户端。宿主创建一个共享工厂，应用在发起请求时获取客户端。

## 普通请求

以下 factory 为宿主注入的工厂：

```rust
let client = factory.client();
let response = common::http_client_helper::get(
    &client,
    "https://api.example.com/items",
    None,
).await?;
let status = response.status();
let body = response.text().await?;
```

提交 JSON：

```rust
let response = common::http_client_helper::post_string(
    &factory.client(),
    "https://api.example.com/items",
    r#"{"name":"示例"}"#.to_owned(),
    "json",
    None,
).await?;
```

辅助方法完整接收正文后返回响应，4xx/5xx 由调用方检查。可用入口包括 get、delete、post_string、post_form、post_form_data。`post_string` 支持 json 和 xml。

默认客户端不保存 Cookie、不跟随重定向，支持 gzip、deflate 和 Brotli 解压；整体请求期限一百秒，连接池空闲期限六十秒，工厂每一百二十秒按需轮换。可通过 HttpClientFactoryOptions 调整。不要长期保存取得的客户端，以便后续调用使用轮换后的池。

## 上传与下载

multipart 使用有序 PostFormDataItem 列表，支持重复字段名：

| 类型 | 内容 |
| --- | --- |
| File | 内存字节 |
| FileStream | reqwest::Body 流 |
| FileHandle | 可定位的 tokio::fs::File，适合签名上传 |

流的所有权交给请求，结束或取消后释放。未知剩余长度传 None，文件名和字段名不能全为空白。

下载使用 `streaming_client()` 和公共 `download_file`，以 `request_timeout()` 作为响应头期限。正文流式写入同目录临时文件，完整后替换目标，失败或取消清理临时文件。它只接受 2xx，不把响应头期限继续用于整段文件传输。

原始流式读取可使用 reqwest 接口；已有 RequestBuilder 需要完整缓冲时使用 `send_buffered`。普通远程上传与 UEditor 抓图规则不同，见[文件接口](webapi-files.md)。

## 命名客户端

客户端宿主默认注册以下名称，名称区分大小写：

| 名称 | 功能 |
| --- | --- |
| SkipSsl | 显式跳过服务端证书校验 |
| HttpSign | 获取并缓存令牌，添加签名头，保存续签令牌 |
| CarryCert | 使用客户端证书发起 TLS 请求，仍验证服务端证书 |

```rust
let client = factory.named_client("HttpSign")?;
let response = client.get("https://api.example.com/items", None).await?;
```

命名客户端提供 get、delete、post_string、post_form、post_form_data 以及接收 reqwest::Request 的 execute。需要只等待响应头时使用 execute_streaming。

未注册的非空名称使用独立默认客户端：HTTP/1.1、Cookie、不自动解压，最多五十次重定向。重定向清除认证头；不可重放的流式正文需要原样重发时返回错误。

## 签名客户端配置

```toml
[http_clients.http_sign]
token_url = "https://auth.example.com/Authorize/GetToken"
name = "service-account"
password = ""
cache_key = "token"
```

密码通过 `APP__HTTP_CLIENTS__HTTP_SIGN__PASSWORD` 提供。缓存未命中时用 name、passWord 字段登录，响应为令牌文本。收到 200 且带 NewToken 的响应时更新缓存，不自动重试失败业务请求。

HttpSign 的 token_url 需接受 name/passWord 登录协议；本项目默认 GetToken 接口使用 userName/password，不能直接将两者当作同一请求模型。接入该接口时需适配登录处理器，或由应用预先将已取得令牌写入同一缓存键。

签名文件上传使用内存文件或 FileHandle，文件从起点提交；不可定位流不支持签名。签名协议见[请求签名](webapi-signing.md)。

## 客户端证书

```toml
[http_clients.carry_cert]
certificate_path = "/run/secrets/client.p12"
password = ""
```

密码使用 `APP__HTTP_CLIENTS__CARRY_CERT__PASSWORD`。相对路径按工作目录解析，证书在首次获取或工厂轮换时加载。配置用于出站身份，与入站 host.tls 独立。

## 请求选项

`common::http_client_helper::RequestOptions` 在进程内按名称携带类型化数据，不生成 HTTP 头。使用 try_add、set 和 get 管理值，通过命名客户端的 `with_request_options` 传入。

响应的 extensions 可读取处理后的 RequestOptions。需要这些处理器行为时使用命名客户端；原生 reqwest 调用不经过命名处理器。
