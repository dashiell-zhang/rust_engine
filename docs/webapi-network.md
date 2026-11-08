# WebAPI 代理与 HTTPS

WebAPI 支持由反向代理终止 TLS，也支持直接加载证书提供 HTTPS。请求处理器可通过 `RequestContext` 获取地址和客户端 IP，用于生成回调链接。

## 反向代理

代理向宿主传递原始 Host、X-Forwarded-For 和 X-Forwarded-Proto。框架只消费 For/Proto 最右侧一跳，不使用 X-Forwarded-Host 或 X-Forwarded-Prefix 改写主机和路径。

转发头对 IPv4、IPv4 映射 IPv6 和 IPv6 回环来源生效。部署时限制直接访问宿主端口，使用[部署生成器](deployment.md)提供的 Nginx 配置作为起点。

业务读取客户端 IP 时依次使用 x-original-forwarded-for、剩余 x-forwarded-for 的第一项，最后回退处理后的连接地址；X-Real-IP 不参与。

```rust
use webapi_core::request_context::RequestContext;

// 将 context: RequestContext 加入处理器参数。
let callback_base = context.base_url();
let client_ip = context.client_ip()?;
```

支付通知地址使用请求上下文，文件访问地址使用 file.public_base_url，二者分别配置。

## 直接 HTTPS 监听

```toml
[host]
listen_address = "0.0.0.0:3000"

[host.tls]
listen_address = "0.0.0.0:3443"
certificate_path = "/certificates/fullchain.pem"
key_path = "/certificates/private.key"
```

证书链先放服务器证书，再放中间证书。私钥支持 PKCS#8、RSA/EC PEM；证书与私钥在同一文件时可省略 key_path。

PFX/PKCS#12 将 certificate_path 指向 pfx 或 p12 文件并省略 key_path。密码通过 `APP__HOST__TLS__PASSWORD` 提供。容器中应挂载证书目录并映射 HTTPS 端口。

host.listen_address 为空时仅监听 HTTPS。相对证书路径按工作目录解析，替换证书后重启。证书无效、密码错误或地址占用会启动失败，不退回明文。

## HTTPS 重定向

```toml
[host]
listen_address = "0.0.0.0:3000"
https_port = 443
```

http 请求返回 307，保留路径和查询串。代理已标记 HTTPS 时不重定向。https_port 只指定跳转目标，不创建 TLS 监听器。

未显式设置时使用 HTTPS_PORT 等环境配置，再使用直接 HTTPS 监听端口；都没有时继续接受 HTTP。

HTTPS 响应添加一年 HSTS，localhost 和回环地址除外，不启用 includeSubDomains 或 preload。

## 控制器路由

端点通过[统一端点声明](host-extensions.md#声明-http-端点)注册。ASCII 路径字面量不区分大小写，允许一个末尾斜杠：

- /User/GetUser
- /user/getuser
- /USER/GETUSER/

上述路径匹配同一端点，路径参数值保持原样。连续或中间斜杠不合并。GET 不隐式支持 HEAD；方法不匹配返回 405 和 Allow。

静态文件使用文件系统大小写规则，Swagger 和健康检查各自保留独立路由行为。

## 请求路径表示

路径只解码一次，保留编码斜杠 %2F，不把路径中的 + 当空格，消除完整的 . 和 .. 段。查询串保持原文。

例如 /A%2FB.txt 表示文件名中包含字面 %2F，/A/B.txt 表示子目录。API 大小写匹配不会改变静态文件名。

签名使用规范化后的 URI 表示及原查询串，不能用路由模板或小写路径替代实际请求，见[签名算法](webapi-signing.md)。
