# WebAPI 代理与 HTTPS

两个 WebAPI 使用同一请求地址上下文。业务处理器可提取 `webapi_core::request_context::RequestContext`，通过 `base_url()` 获取协议、主机和显式端口，通过 `client_ip()` 获取客户端 IP。支付接口的通知地址从该上下文生成，文件存储的 `public_base_url` 只用于文件访问。

## 控制器路由

业务 Feature 使用 `webapi_core::routes::WebApiRoutes`，方法由同模块的 `get`、`post`、`delete` 声明。它们同时登记实际处理器、路径模板和允许的方法；OpenAPI 与权限清单不充当路由来源，因此未生成文档的 UEditor 也遵循相同规则。多方法端点可用 `get(handler).post(handler)`；中间件通过 `map` 装配到内部 Axum 方法路由，接口守卫使用 `route_layer`。

当前控制器模板由 ASCII 字面量段和完整的命名参数段组成。字面量匹配不区分大小写，允许一个末尾斜杠，不折叠连续或中间斜杠。`/User/GetUser`、`/user/getuser`、`/USER/GETUSER/` 选择同一个端点；匹配完成后立即恢复完整原始传输 URI；框架按下述共享路径表示执行签名、缓存和日志，不使用路由模板替换实际路径。参数段保持原值，例如支付回调的商户标识不会被改成小写。仅大小写不同的端点声明会在启动时报错，不通过别名隐藏冲突。

控制器 GET 不隐式开放 HEAD。路径存在但方法不允许时返回空体 405 和声明的方法列表，不运行该接口的认证、签名、缓存或队列守卫；CORS 预检仍优先返回 204，不附带方法路由自动生成的 Allow。`/healthz` 保持支持任意方法，公开静态文件保留文件系统大小写行为；Swagger 不套用控制器模板匹配。

匹配必须位于内层 Router 之外，普通 Axum layer 在路径选择后才执行，不能用它来改写用于选择端点的地址，参见 [Axum 中间件说明](https://docs.rs/axum/latest/axum/middleware/#rewriting-request-uri-in-middleware)。ASP.NET 的[路由文档](https://learn.microsoft.com/en-us/aspnet/core/fundamentals/routing?view=aspnetcore-10.0)规定按解码后的路径匹配；本宿主使用下面的共享路径表示，更复杂的路由模板仍未支持。

## 请求路径表示

`request_path` 在路由之前只解码一次路径，保留 `%2F`/`%2f`，不把 `+` 当空格。有效 UTF-8 恢复为字符，非法、过长、代理区或截断 UTF-8 编码保持原文；编码 NUL 返回空体 400。解码后消除完整的 `.`/`..` 段，保留中间空段；重复编码不会再次展开。这些规则对应 [Kestrel PathDecoder](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Servers/Kestrel/Core/src/Internal/Http/PathDecoder.cs)。

原始传输 URI 仍可在请求中读取，但框架按用途使用共享表示：路由、缓存键和静态文件使用解码后的路径；签名、日志完整地址和 HTTPS 重定向使用 [PathString 的 URI 表示](https://github.com/dotnet/aspnetcore/blob/release/10.0/src/Http/Http.Abstractions/src/PathString.cs)，保留合法百分号转义并重新转义其他字符。查询串始终保持原文。交给 Axum 匹配的临时 URI 单独转义全部百分号，防止路径参数再被解码，选中端点后恢复原始传输 URI。

例如 `/A%2FB.txt` 访问名为 `A%2FB.txt` 的文件，而 `/A/B.txt` 访问子目录文件；`/A%252FB.txt` 只解码成前者，不成为子目录。`/x/%2e%2e/CaseFile.txt` 归一化到 `/CaseFile.txt`。不因 API 大小写匹配而改变静态文件名的大小写。

## 反向代理

宿主默认以 HTTP 监听，可以由前置代理终止 TLS，也可以配置下述直接 HTTPS 监听。代理向宿主传递原始 `Host`、`X-Forwarded-For` 和 `X-Forwarded-Proto`。框架只消费 For/Proto 最右侧的一跳，不用 `X-Forwarded-Host` 或 `X-Forwarded-Prefix` 改写请求主机与路径。

信任范围沿用 NetEngine：IPv4 来源、IPv4 映射 IPv6 来源和 IPv6 回环地址。其他 IPv6 来源的转发头不生效。消费后的连接和协议原值保存在 `X-Original-For`、`X-Original-Proto`，剩余代理链继续保留。

客户端 IP 的业务读取顺序为 `x-original-forwarded-for`、剩余 `x-forwarded-for` 的第一项，最后回退代理处理后的连接地址；IPv4 映射地址在回退时转为 IPv4 文本。`X-Real-IP` 不参与这套规则。

## 直接 HTTPS 监听

在任一 WebAPI 的配置中添加可选 `host.tls`。例如同时监听 HTTP 和 HTTPS：

```toml
[host]
listen_address = "0.0.0.0:3000"

[host.tls]
listen_address = "0.0.0.0:3443"
certificate_path = "/certificates/fullchain.pem"
key_path = "/certificates/private.key"
```

证书链按服务器证书在前、中间证书在后的顺序放入 PEM/CRT 文件。私钥支持 PKCS#8、RSA/EC PEM；证书和私钥放在同一 PEM 文件时可以省略 `key_path`。加密私钥的密码通过 `APP__HOST__TLS__PASSWORD` 提供。

PFX/PKCS#12 使用同样配置，将 `certificate_path` 指向 `.pfx` 或 `.p12` 文件，省略 `key_path`，通过 `APP__HOST__TLS__PASSWORD` 提供密码。无密码的 PFX 可以不配置密码。

`host.listen_address` 设置为空字符串可以仅监听 HTTPS。证书路径相对于程序工作目录，也可使用绝对路径；容器部署时将证书目录挂载到容器，并映射配置的 HTTPS 端口。证书在启动时读取，替换后重启宿主，不会自动生成开发证书或修改本机信任库。

直接连接支持 TLS 1.2/1.3，通过 ALPN 协商 HTTP/2 或 HTTP/1.1。证书或私钥无效、密码错误、监听地址占用时启动失败；不会静默退回 HTTP。HTTP 和 HTTPS 共享宿主停机期限。

## HTTPS 重定向

可以在宿主配置中指定外部 HTTPS 端口：

```toml
[host]
listen_address = "0.0.0.0:3000"
https_port = 443
```

也可以设置 `APP__HOST__HTTPS_PORT`。未显式配置时依次读取 `HTTPS_PORT`、`DOTNET_HTTPS_PORT`、`ASPNETCORE_HTTPS_PORT`，再读取对应的 `ANCM_HTTPS_PORT` 配置，最后采用直接 HTTPS 监听的实际端口。未找到端口时继续接受 HTTP，每个宿主实例只记录一次端口未确定的消息。

HTTP 请求返回 307，保留路径与查询串，替换原 Host 中的端口；443 不出现在目标 URL 中。代理已将协议标记为 HTTPS 时不重定向。此设置不会创建 TLS 监听器或加载证书。

## HSTS 与开发文档

HTTPS 请求返回 `Strict-Transport-Security: max-age=31536000`，不添加 `includeSubDomains` 或 `preload`。`localhost`、`127.0.0.1`、`[::1]` 不添加该头。普通业务错误保留 HSTS，进入全局异常处理的响应清除它。

Development 的 Swagger GET 在公共 HTTPS 管线之前直接响应；普通接口、静态资源及文档预检仍遵循公共管线。生产响应压缩和 CORS 可以与 HSTS 同时生效。
