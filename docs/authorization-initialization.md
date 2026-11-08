# 认证、权限与管理员初始化

## 宿主环境与错误响应

WebAPI 启动时依次读取 `APP_ENVIRONMENT`、`DOTNET_ENVIRONMENT`、`ASPNETCORE_ENVIRONMENT` 中第一个非空值，默认使用 Production，并加载相应的 `appsettings.{环境名}.toml`。运行环境与 Debug/Release 编译配置独立。Swagger 页面 `/swagger` 和文档 `/swagger/v1/swagger.json` 仅在 Development 环境开放。

JWT 认证失败返回空体 HTTP 401，并带 `WWW-Authenticate: Bearer`；提交的令牌校验失败时，挑战头增加 `error="invalid_token"`，部分失败原因通过 `error_description` 描述。请求签名摘要不匹配或过期仍返回 JSON 401，不附加认证挑战头。已认证但没有路由权限返回空体 403。

业务校验失败使用相应的业务错误信息。内部异常在 Development 返回 HTTP 500 和诊断信息，在其他环境返回 HTTP 400 与 `{"errMsg":"系统全局内部异常"}`，同时禁止缓存该异常响应。内部异常不保留此前生成的 NewToken；正常权限拒绝、签名不匹配和业务校验失败仍可带续签头。

## 请求签名

管理端启用签名的接口在 Release 构建中要求 `Authorization: Bearer <JWT>`、`Time`（Unix 毫秒）和 `Token` 请求头。受保护接口先进行认证、续签和权限判断，再检查签名；因此签名失败的响应也可能带有 `NewToken`。Debug 构建跳过签名检查。

签名原文为 JWT 第三段、Time 原始文本、路径及查询串、请求体签名内容的顺序拼接，Token 使用 UTF-8 原文的 SHA-256 大写十六进制摘要。时间判断为 `Time + 3 分钟 > 当前时间`，不额外限制未来时间。

普通请求体按 UTF-8 文本读取，支持 UTF-8、UTF-16 和 UTF-32 BOM；`Content-Encoding: gzip` 时先解压。表单字段按名称排序，同名 ASCII 字段不区分大小写、保留首次名称，多个值用逗号连接；multipart 在字段之后拼接按字段名排序的各个文件字段名及文件内容摘要。请求体签名读取不会替换后续处理器收到的原始字节。

## 管理员初始化

先使用 `repository_tool` 完成业务数据库迁移，再以 `APP_ENVIRONMENT=Development` 启动管理端，调用 `POST /Authorize/InitData`，不需要请求体。接口仅在 Development 环境执行；初始化不会随迁移或宿主启动自动运行。

数据来自 [InitData/AuthorizeInitData.json](../InitData/AuthorizeInitData.json)。构建时复制到管理端可执行文件旁的 `InitData` 目录，Linux 发布包和 Docker 导出目录同样携带此文件。运行时按可执行文件位置查找，不依赖当前目录。调用前修改文件中的管理员账号和密码；重新构建会重新复制工作区内的文件。

初始化在一个事务中执行：

- 按用户名更新或恢复管理员，按角色编码更新或恢复管理员角色，保留已有记录主键。
- 按 `Sign` 更新功能；`ParentSign` 引用同一文件里的功能，可以先声明子级再声明父级。
- 按 `Module` 与 `Route` 更新路由，`FunctionSign` 为 null 时不绑定功能。
- 恢复管理员角色关系，软删除该用户的其他有效角色关系。
- 为管理员角色授予数据库中全部有效功能，包括文件外已有的功能。

重复执行会恢复软删除记录，并将管理员密码重新设置为文件中的密码。引用未定义的功能或写入失败时整次回滚。JSON 字段名不区分大小写。返回管理员用户名、角色编码、文件内功能与路由数量、有效授权与角色关系数量，以及读取的文件路径。

权限模块默认使用 `Admin.WebAPI` 和 `Client.WebAPI`，权限表的路由键使用 `User/GetUserList` 这类无前导斜杠的模板；实际 HTTP 地址仍为 `/User/GetUserList`。路由注册或删除后，在对应宿主显式调用 `GET /Authorize/UpdateRoute`，只同步该宿主模块中需要鉴权且未标记匿名的路由，再配置对应的功能绑定。两端此入口按源控制器匿名开放，成功返回 200 空正文。宿主启动不改写权限表。初始化文件原样保留 NetEngine 当前数据中的其他模块记录，只有与宿主模块和路由键相符的记录参与当前请求授权。

同步查询只读取有效记录，整批更新使用一个事务与原行版本。保留已有有效路由的功能绑定，只更新备注；不再出现的路由软删除，空清单也会清理当前模块。已删除的路径重新注册时创建新 ID 且不带旧功能绑定，旧记录继续保持删除状态。同步不额外写入 DataUpdateLog；重复同步没有变化时仍返回 200 空正文。

## 权限读取与分配

路由授权与当前用户功能列表读取当前数据库状态，不缓存结果。路由没有有效记录或没有绑定功能时，权限检查放行；需要登录的接口仍要求有效令牌。已绑定路由检查用户直接授权与角色继承授权，判断依据是有效的授权记录。功能列表只返回有效功能，`sign` 按完整值匹配。

`GetUserFunction` 和 `GetRoleFunction` 返回模块树，模块的 `childList`、`functionList` 是数组，功能叶节点的两个集合为 null。查询尚无分配关系的用户或角色也可取得可选功能树。

`SetUserFunction` 在未获得直接或继承授权时添加直接授权；取消时只处理查询到的一条授权，来自角色的授权不能在用户页面单独取消，应调整角色功能或用户角色关系。取消不存在的分配关系返回成功。角色下仍有有效用户角色关系时，删除角色返回 HTTP 400。

## 登录与令牌续签

用户名密码登录按完整用户名匹配，不裁剪非空用户名两端的空白；错误凭据返回 HTTP 400。签发的 JWT 使用 `userId`、`tokenId` 字符串声明和标准的 `iss`、`aud`、`iat`、`nbf`、`exp` 字段。验证时不额外要求 iat，支持数组形式的 aud 以及数字形式的业务 ID。宿主验证 ES256 签名、颁发者、受众和有效时间；有效期校验采用源项目所用 IdentityModel 的 [默认 5 分钟时钟容差](https://learn.microsoft.com/en-us/dotnet/api/microsoft.identitymodel.tokens.tokenvalidationparameters.defaultclockskew)，同时拒绝 nbf 晚于 exp 的令牌。

令牌有效期过半后，宿主在权限判断和业务处理之前尝试续签，通过 `NewToken` 响应头返回，并公开该跨域响应头。客户端收到后保存并用于后续请求。已认证但无权限的请求返回 403，也可能带有续签头；无效令牌返回 401。续签失败会返回错误，不继续执行业务。

同一旧令牌通过 `IssueNewToken{tokenId}` 非阻塞锁协调，缓存键为 `{tokenId}newToken`，十分钟内复用已签发的新令牌。多个实例需要共享缓存和锁后端及其前缀。每次首次签发续期令牌时，尝试取得 `ClearExpireToken` 锁并物理删除创建时间早于七天的有效令牌记录；已有软删除记录不参与这次清理。

`UserToken` 保存签发与前驱关系，不作为 JWT 黑名单。清理这些记录本身不会使已签名 JWT 立即失效。旧密码改密不清理令牌记录；短信改密在一个事务中更新密码并物理删除该用户的有效令牌记录，验证码仍在原有效期内可用。删除用户仅软删除用户，不附加清理令牌记录；后续请求仍受 JWT 有效期、路由权限和具体用例的数据查询约束。
