# 认证、权限与管理员初始化

认证模块提供登录、JWT 续签和路由权限控制。管理端支持从初始化文件创建管理员，接口与功能绑定后可按用户或角色分配访问权限。

## 初始化管理员

1. 完成业务数据库迁移。
2. 修改 [InitData/AuthorizeInitData.json](../InitData/AuthorizeInitData.json) 中的账号、密码、角色和功能。
3. 重新构建并以 Development 环境运行管理端。
4. 调用初始化接口：

```powershell
curl.exe -X POST http://127.0.0.1:3000/Authorize/InitData
```

接口无需正文，只在 Development 执行。文件在构建时复制到可执行文件旁的 InitData 目录，发布包也携带此文件。初始化不随迁移或启动自动运行。

初始化在一个事务中更新或恢复管理员、角色、功能和路由。重复执行会重置管理员密码为文件值，并将管理员角色授权到全部有效功能。引用未定义功能或保存失败时整次回滚。

## 登录与续签

用户名密码登录使用 `POST /Authorize/GetToken`，请求示例：

```json
{"userName":"admin","password":"填写初始化文件中设置的密码"}
```

成功后保存返回的 JWT，请求受保护接口时发送：

```http
Authorization: Bearer <JWT>
```

令牌使用 ES256 签名，宿主校验颁发者、受众和有效时间，时钟容差为五分钟。多个实例应使用一致的 JWT 配置。

有效期过半后尝试续签，通过 `NewToken` 响应头返回。客户端收到后用于后续请求，该头也向跨域客户端公开。多个实例共享缓存和锁，避免同一旧令牌反复签发。

UserToken 记录签发关系，不作为 JWT 黑名单；删除记录不意味着已签名 JWT 立即失效。

## 路由与功能权限

默认模块为 Admin.WebAPI 和 Client.WebAPI。添加或移除端点后，在对应宿主显式调用：

```powershell
curl.exe http://127.0.0.1:3000/Authorize/UpdateRoute
curl.exe http://127.0.0.1:3001/Authorize/UpdateRoute
```

成功为 200 空正文，两端此入口匿名开放。同步只更新当前模块的权限端点，宿主启动不自动同步。

路由键不含前导斜杠，例如 `User/GetUserList`。有效路径保留功能绑定，移除路径软删除；已删除路径重新出现时创建新记录，需要重新绑定。

没有绑定功能的路由通过权限检查，但受保护接口仍要求登录。绑定功能后根据用户直接授权或角色授权判断。用户和角色功能树可通过管理端权限接口读取和设置。

角色继承的权限不能在用户页面单独取消，应调整角色功能或用户角色关系。角色下仍有有效用户关系时不能删除角色。

## 请求签名与环境

管理端声明签名的端点在 Release 构建中额外校验签名，Debug 构建跳过。签名要求和算法见[请求签名](webapi-signing.md)。

运行环境按 APP_ENVIRONMENT、DOTNET_ENVIRONMENT、ASPNETCORE_ENVIRONMENT 选择，默认 Production。运行环境与编译模式独立；Swagger 只在 Development 提供。

## 响应处理

| 情况 | 响应 |
| --- | --- |
| JWT 无效或缺失 | 空体 401，带 WWW-Authenticate |
| 已登录但无权限 | 空体 403 |
| 签名不匹配或过期 | JSON 401 |
| 业务校验失败 | 相应业务错误 |
| 内部异常，Development | 500 和诊断信息 |
| 内部异常，其他环境 | 400 和系统全局内部异常 |

续签先于权限及业务执行，403 或签名失败响应也可能带 NewToken。内部异常响应不保留续签头。
