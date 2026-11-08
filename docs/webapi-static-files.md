# WebAPI 静态文件

管理端将 `[file].root_path` 作为公开 Web 根目录，默认是工作目录下的 `wwwroot`。例如 `wwwroot/images/logo.png` 通过 `/images/logo.png` 访问，上传文件保存在该目录下的 `uploads/年/月/日`。文件存储选择云组件时，Web 根目录仍然开放。

客户端不开放静态文件服务，文件根目录默认为工作目录 `.`。客户端文件访问通过已注册的文件接口处理，接口语义见 [文件上传与下载](webapi-files.md)。两端可以通过 `APP__FILE__ROOT_PATH` 覆盖文件根目录；部署时按宿主分别设置。已有文件保留在原目录时，应显式指定对应目录，框架不自动移动数据。

静态服务只响应 GET 和 HEAD，已注册 API 优先匹配。目录不会自动返回 `index.html`，需要请求具体文件路径；未知扩展名、点文件与点目录不公开。公开类型采用 ASP.NET Core 默认扩展名表，保留其 MIME 类型；映射表的许可证位于 `project_core/webapi_core/ASP.NET-Core-LICENSE.txt`。

响应提供 ETag、Last-Modified 和单范围下载，支持条件请求。静态响应经过宿主统一的 CORS、HTTPS 策略和响应压缩管线。文件位于公开根目录时按静态路径访问，不进行文件记录的权限判断；`GetFile` 和 `GetImage` 同样允许匿名读取本地文件，管理端仍执行请求签名；`isPublicRead` 不作为这些本地路径的访问校验。

默认运行数据目录 `wwwroot/uploads`、`wwwroot/temps`、`uploads`、`temps` 不纳入 Git 与 Docker 构建上下文。公开资产由部署方放到管理端 Web 根目录，不包含 Admin.App 前端。
