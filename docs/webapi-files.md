# 文件上传与下载

文件模块统一管理本地、阿里云 OSS 和腾讯云 COS 文件，保存文件记录并提供上传、下载和访问地址接口。配置方式见[组件配置](component-configuration.md#文件存储)。

## 上传文件

两端提供 `POST /File/UploadFile`，使用 multipart 的 file 字段，并传入 business、sign、可空 key 和 isPublicRead。

客户端调用示例，JWT 为登录取得的令牌：

```powershell
curl.exe -X POST "http://127.0.0.1:3001/File/UploadFile?business=demo&sign=attachment&isPublicRead=false" -H "Authorization: Bearer <JWT>" -F "file=@C:/files/example.pdf"
```

成功返回 JSON 字符串文件 ID。管理端 Release 还需[请求签名](webapi-signing.md)。

普通上传读取第一个名为 file 的文件字段，忽略字段名大小写。缺少文件或文件为空会返回错误；非 multipart Content-Type 返回 415。接口不设置额外固定上传上限，反向代理可另行限制。

临时文件在根目录的 temps，正式文件位于 uploads/年/月/日。对象存储名称与文件记录使用独立 ID，原上传名用于下载附件名。

## 下载、地址与删除

| 接口 | 用途 |
| --- | --- |
| GET /File/GetFile?fileId=... | 下载本地原文件 |
| GET /File/GetImage?fileId=...&width=...&height=... | 原图或缩放 PNG |
| GET /File/GetFileUrl?fileId=...&isInline=... | 公开地址或十分钟云临时地址 |
| DELETE /File/DeleteFile?id=... | 软删除记录，不删除物理文件或云对象 |

GetFile、GetImage 允许匿名访问，管理端仍检查签名。其他文件接口需要宿主认证和权限。`isPublicRead` 控制云权限及地址生成，不是本地下载的权限开关。

GetFile 以保存的文件名作为附件名，提供文件长度和修改时间，不支持范围下载，不设置 ETag。云文件不会自动下载到本地公开目录，应通过 GetFileUrl 访问。

宽高均为零或指定边超过原图时返回原文件；只指定一边时按比例缩放，两边均指定则使用指定尺寸。变换结果为 PNG。

公开地址直接拼接 public_base_url 与相对路径，请在基地址中配置所需末尾斜杠。私有地址生成失败返回错误，不自动退为公开地址。

## 云存储使用要求

阿里云与腾讯云均以远程目录加本地存储文件名作为对象键，原始展示名只用于 Content-Disposition。云对象访问权限由 isPublicRead 决定。

- OSS 上传、下载按服务端摘要检查内容；下载重试会重新覆盖目标，最终失败可能保留部分内容。直接使用组件下载时预先准备父目录。
- COS 大文件上传会检查同名未完成上传并校验分片，需要列出上传、列出分片和上传权限。普通失败保留服务端分片会话。
- COS 下载达到 20 MiB 时分片并发下载，成功后合并；失败可能保留临时分片。不要并发下载到相同目标路径。
- 上传期间不要修改源文件。业务软删除不会调用云组件的物理删除方法。

直接调用 `AppFileStorage::get_file_url` 时，有效期使用 `chrono::TimeDelta`；通常应提供正时长。OSS 最长七天，COS 非正秒数回退到默认 600 秒密钥区间。

## 远程上传

客户端提供 `POST /File/RemoteUploadFile`：

```json
{
  "business": "demo",
  "key": null,
  "sign": "attachment",
  "isPublicRead": false,
  "fileName": "example.pdf",
  "fileUrl": "https://files.example.com/example.pdf"
}
```

需要认证和权限，成功返回文件 ID。来源可以是宿主可访问的 HTTP/HTTPS 地址，包括内网；不自动跟随重定向。非 2xx 或下载失败返回 400 与“远程文件下载失败”。接口没有额外固定下载上限，空响应可保存为空文件。

## UEditor

管理端 `/Ueditor/ProcessRequest` 支持 GET/POST，需要认证和权限，不要求签名，也不出现在公开接口文档中。

- action=config 获取配置及允许的扩展名。
- uploadimage、uploadvideo、uploadfile 使用 upfile 文件字段。
- uploadscrawl 使用 upfile 普通字段中的 Base64 文本，不接受 data URL。
- catchimage 使用重复的 source[] 字段提交来源。
- 除 config 外均要求正整数 uploadKey。

上传结果是包含 JSON 的文本，HTTP 200 也可能代表业务失败，应检查 state 是否为 SUCCESS。

抓图只允许公网 HTTP/HTTPS，校验 DNS 和每次重定向目标，最多五次重定向，不允许 HTTPS 降级。来源扩展名、image/ 响应类型及最大大小须满足 config 中的规则。各来源独立返回结果。

静态目录公开范围及条件下载见[静态文件](webapi-static-files.md)。
