# 对外 HTTP 调用

共享工厂位于独立组件 `infrastructure/http_client`，导入路径为 `http_client::HttpClientFactory`，不依赖应用上下文、SeaORM 或 JWT。

应用服务使用上下文中的 `http_client_factory` 获取客户端，由宿主创建和共享工厂。不要在每次调用时创建新工厂，也不要长期保存工厂返回的客户端；在需要发出请求时重新获取，才能使用轮换后的连接池。

默认客户端不跟随重定向、不保存 Cookie，支持 gzip、deflate 和 Brotli 解压。普通客户端有 100 秒整体请求期限，连接池空闲期限为 60 秒，工厂每 120 秒按需轮换。可以通过 `HttpClientFactoryOptions` 调整这些选项。

WebAPI 的资源工厂现在直接接收已配置的 `Arc<HttpClientFactory>`，文件提供商与 LLM 默认客户端工厂复用该对象。LLM 应用服务通过注入的 `LlmClientFactory` 创建客户端，不在应用内部固定调用默认协议工厂。具体替换方式见 [宿主资源与 LLM 装配](database-modules.md)。

## 普通请求

`common::http_client_helper` 提供 `get`、`delete`、`post_string`、`post_form` 和 `post_form_data`。这些方法接收工厂提供的 `reqwest::Client`，完整接收响应正文后返回 `reqwest::Response`。请求失败、正文读取失败或超过 2,147,483,647 字节的缓冲上限时返回错误；HTTP 4xx/5xx 响应仍返回给调用方，由业务决定处理方式。

- `post_string` 的类型参数只接受忽略大小写的 `json` 或 `xml`，分别使用 `application/json; charset=utf-8` 和 `text/xml; charset=utf-8`。
- `post_form` 使用 `application/x-www-form-urlencoded; charset=utf-8`；空格编码为 `+`，文字按 UTF-8 百分号编码。
- `post_form_data` 接收有序的 `PostFormDataItem` 列表，允许重复字段名。字段名和文件名不能全为空白；文本值允许空字符串。
- multipart 文本项使用 `text/plain; charset=utf-8`，文件项不附加 Content-Type。`File` 持有内存字节；`FileStream` 持有 `reqwest::Body`，不必将整个文件预读到内存；`FileHandle` 持有可重新定位的 `tokio::fs::File`，供签名上传使用。`length` 是当前位置起的剩余长度，未知时传 `None`。

上传流的所有权交给请求，发送结束、失败或取消后释放。调用方取消时应丢弃请求 Future；如果自行创建后台任务，应同时管理任务取消，不能仅丢弃任务句柄。

已经组装的 `reqwest::RequestBuilder` 可以交给 `send_buffered`，采用相同的完整正文返回语义。需要自定义逐块读取的调用方可以使用 reqwest 自身的流式响应接口。

自定义请求头按头名称忽略大小写处理：普通请求头追加值，标准内容头替换值。没有正文时忽略内容头，无效的头名称也会忽略；不能被 HTTP 头值类型表示的值返回错误。

## 文件下载

文件下载使用 `streaming_client()` 和 `download_file`，并传入工厂的 `request_timeout()` 作为响应头期限。正文按流写入文件，不采用普通请求的完整正文内存缓冲，也不将响应头期限继续套用到正文传输。

`download_file` 返回实际保存路径，只接受 2xx 响应；先写同目录临时文件，完整写入后再替换目标文件，失败和取消会清理未完成的临时文件。调用方若还需将下载结果转交业务存储，应继续持有临时文件清理守卫，覆盖保存失败或取消。

普通远程上传与 UEditor 抓图使用不同规则，详见 [文件接口](webapi-files.md)。不要把 UEditor 的公网地址限制、重定向和图片校验规则套用到普通 HTTP 工厂。

## 客户端宿主的命名客户端

客户端宿主在同一个工厂中注册以下名称，名称区分大小写；管理端与任务宿主不自动注册它们。

| 名称 | 用途 |
| --- | --- |
| `SkipSsl` | 显式忽略服务端证书校验的调用；普通客户端的校验策略不受影响 |
| `HttpSign` | 获取并缓存登录令牌，添加 Authorization、Token 和 Time，保存响应中的续签令牌 |
| `CarryCert` | 使用配置的 PKCS#12 客户端证书和私钥发起 TLS 请求，仍验证服务端证书 |

通过 `state.http_client_factory.named_client("HttpSign")?` 获取客户端。返回对象提供 `get`、`delete`、`post_string`、`post_form`、`post_form_data` 和接收 `reqwest::Request` 的 `execute`；这些入口均经过注册的处理器，普通调用完整接收正文后返回。`execute_streaming` 在处理完响应头后返回，后续正文由调用方读取和取消。

命名客户端与处理器按名称共享、惰性创建，并遵循工厂的 120 秒轮换期限。获取时创建失败会返回错误。每次新操作应从工厂获取，不要永久保存返回对象。空名称使用普通客户端规则。

未注册的非空名称会创建独立默认客户端：使用 HTTP/1.1、保存 Cookie、不自动解压，最多跟随 50 次重定向。达到上限、缺少有效 Location 或 HTTPS 降级时返回当前响应。300/301/302 的 POST 和 303 的非 GET/HEAD 请求会转为无正文 GET；每次重定向清除 Authorization 与 Proxy-Authorization。Cookie 按目标地址选择，调用方明确设置的 Cookie 与容器中的 Cookie 合并。同名句柄共享 Cookie，轮换时创建新的 Cookie 容器。

当前自动重定向能够重放内存请求正文；不可克隆的正文在需要原样重发时返回错误。尤其是 reqwest multipart 编码流，即使底层文件可定位，目前仍不能自动重放。需要重定向的流式上传还不能据此按完整等价能力使用。

## 请求选项

`common::http_client_helper::RequestOptions` 以区分大小写的名称保存进程内任意类型值，供宿主出站处理器使用，不生成 HTTP 请求头。`try_add` 保留已有值，`set` 显式覆盖，`get::<T>` 按类型读取。

取得命名客户端后调用 `with_request_options(options)`，再使用普通发送入口。每个请求复制选项容器，multipart 准备、请求处理和响应处理使用同一份选项；处理器对容器的增删和替换不影响下一次发送。值本身通过共享引用保存，若值带内部可变状态，其共享生命周期由调用方管理。

响应的 `extensions().get::<RequestOptions>()` 可读取处理后的选项，完整正文缓冲会保留它。直接使用原生 reqwest 客户端的公共帮助方法不会经过命名处理器；需要处理器选项时使用工厂返回的命名客户端。

## 签名与证书配置

客户端的配置节如下，密码可分别通过 `APP__HTTP_CLIENTS__HTTP_SIGN__PASSWORD` 和 `APP__HTTP_CLIENTS__CARRY_CERT__PASSWORD` 提供：

```toml
[http_clients.http_sign]
token_url = "https://auth.example.com/Authorize/GetToken"
name = "service-account"
password = ""
cache_key = "token"

[http_clients.carry_cert]
certificate_path = "/run/secrets/client.p12"
password = ""
```

`HttpSign` 使用当前宿主的分布式缓存。缓存未命中时，以 `name` 和 `passWord` JSON 字段调用 `token_url`，成功响应正文应为令牌文本。仓库没有预设登录密码；先配置实际地址及凭据，或由应用预先写入相同缓存键。响应为 200 且含非空 `NewToken` 时更新缓存，不自动重试失败的业务请求。

签名包含令牌最后一段、毫秒时间戳、请求路径及查询字符串。JSON 请求加入解码后的正文；multipart 先按字段名排列文本，再按字段名排列文件摘要。签名文件上传使用 `post_form_data` 的内存文件或 `FileHandle`，从文件起点开始提交；普通非可定位的流会被拒绝。文件摘要完成后复位到起点再发送，请求结束或取消时释放文件。其他媒体类型按当前出站处理器规则不加入正文。

`CarryCert` 的相对路径基于进程工作目录。证书在第一次获取该客户端或连接池轮换时加载，未使用该客户端时无需提供文件。密码错误、缺少私钥或证书与私钥不匹配都会使获取失败，错误消息不包含密码。该配置用于出站客户端身份，与 `host.tls` 的入站监听证书独立。
