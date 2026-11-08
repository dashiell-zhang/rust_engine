# B2 Release 鉴权、缓存与权限同步：结果记录

日期：2026-11-08。范围来自 [阶段清单 B2](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。本轮没有发现需要修复的生产缺陷，仅补充验收记录；B3 未开始。空清单同步仍缺实际入口验证，不标记为全部场景通过。

## 基线与执行方式

- Rust 基线为 `0ce43aed8b76cd130ed713819e8336706f0335a8` 加 B1 尚未提交的两个生产改动，见 [B1 报告](D:/ZWork/rust_engine/docs/queue-b1-review.md)。本轮没有改变生产代码、依赖或迁移定义，前序文档也保留。
- NetEngine 基线为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 加阶段清单记录的五处授权修改。本轮对照源代码，没有修改 NetEngine，也没有运行 .NET 宿主作同步对照。
- 所有 Rust 构建和宿主运行均在 Docker dev 内进行。两个临时基础设施容器为 `rust-engine-b2-postgres` 和 `rust-engine-b2-redis`，使用 latest 镜像和独立数据，不访问原有数据库。
- 通过正式 repository_tool 初始化隔离 business 模型。管理端使用 Release/Development 仅完成 InitData 与 GetToken；随后停止并切换到 Release/Production。以下鉴权、缓存、故障和同步验收均在两个 Release/Production 宿主执行，端口为容器内 3910/3911。
- 为核对续签，基于正式 GetToken 返回的真实用户和令牌记录，使用容器已有 OpenSSL 与隔离配置中的签名密钥重新签发已过半生命周期、但尚未到期的令牌；仅调整 iat/nbf/exp，不替换验证器或业务服务。密钥、令牌和隔离配置仅位于容器临时目录，未输出或纳入仓库。
- HTTP 请求调用现有正式入口，SQL 只用于准备隔离状态、故障条件和读取结果。没有增加测试代码、依赖、专用入口、辅助服务或模拟提供商。

## 四项结果

| B2 项目 | 本轮结论 | 实际证据/边界 |
| --- | --- | --- |
| 1. 认证、续签与权限顺序 | 所列场景通过 | 缺少或非法 JWT 返回空体 401 和 WWW-Authenticate；有效 JWT 无权限返回空体 403，保留 NewToken；权限拒绝先于错误签名 |
| 2. 签名成功/失败及内部异常的续签头 | 所列场景通过 | 合法签名 200、签名不匹配 401 均带 NewToken；非法 Time 与有效签名后的业务查询异常返回生产全局错误，均不带续签头 |
| 3. 客户端 60 秒缓存与 Authorization 隔离 | 所列场景通过 | 两接口均命中旧业务值；相同 JWT、不同 Authorization 原文取得新业务值；缓存未过期时撤销权限得到 403；到期后原头回源 |
| 4. UpdateRoute 原子性与路由重建 | 部分运行通过，空清单仅源码核实 | 人为写入失败后整批回滚、正常同步、模块隔离、软删除重建不恢复旧功能绑定均有实际证据；固定宿主没有提交空路由清单的正式入口 |

## 认证、签名与异常响应

管理端使用 `/User/GetUserList`；请求签名按正式协议计算，请求体为空。涉及续签的请求使用已过半但仍有效的令牌。

| 请求条件 | 状态/正文 | NewToken | 其他响应头 |
| --- | --- | --- | --- |
| 无 Authorization，也无签名头 | 401，空体 | 无 | 有 WWW-Authenticate |
| 非法 JWT | 401，空体 | 无 | 有 WWW-Authenticate |
| 有权限，有效 Time，错误 Token 摘要 | 401，`非法 Token` | 有 | Access-Control-Expose-Headers 包含 NewToken |
| 有权限，Time 为非法文本 | 400，`系统全局内部异常` | 无 | 无 NewToken 暴露声明 |
| 有权限，签名正确 | 200，业务结果 | 有 | 暴露 NewToken |
| 撤销对应功能授权，同时提供错误签名 | 403，空体 | 有 | 暴露 NewToken；没有进入签名错误响应 |
| 有权限，签名正确，但业务查询目标暂不可用 | 400，`系统全局内部异常` | 无 | 无 NewToken 暴露声明 |

最后一项通过在隔离数据库内短暂重命名 User 表触发真实应用查询错误，随后立即恢复。续签复用已存在的续签缓存，权限查询涉及的表保持可用，因此请求确实进入有效签名后的业务查询。没有把签名头解析异常当成唯一的业务内部异常证据。

对应源码：

- Rust [auth.rs](D:/ZWork/rust_engine/project_core/webapi_core/src/auth.rs) 先续签再判断权限，`with_new_token` 遇到 UnhandledError 标记不添加续签头；[sign_verify.rs](D:/ZWork/rust_engine/project_core/webapi_core/src/sign_verify.rs) 与 [error.rs](D:/ZWork/rust_engine/project_core/webapi_core/src/error.rs) 区分签名拒绝和未处理异常。
- NetEngine [PermissionService.cs](D:/ZWork/NetEngine/Presentation/Admin.WebAPI/Services/PermissionService.cs) 在检查功能权限前调用续签；[SignVerifyFilter.cs](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Filters/SignVerifyFilter.cs) 在 Release 执行摘要校验；[GlobalError.cs](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Libraries/GlobalError.cs) 定义生产全局错误。

这里只验证指定正常/拒绝/异常路径；没有扩展到全部 JWT 算法边界、全部 CORS 来源、HTTP/2 或其他宿主中间件。

## 客户端缓存

入口为 `/User/GetUser` 与 `/Authorize/GetFunctionList?sign=user`。先将两个 Client.WebAPI 路由绑定到同一有效功能，并使用既有管理员授权；随后调用接口建立缓存。

1. 缓存初始业务值为用户名称“管理员”和功能名称“用户管理”。修改隔离数据库中的这两个名称后，使用原 `Authorization: Bearer <jwt>` 仍返回旧名称。
2. 使用相同 JWT，仅把头原文改为 `Authorization: bearer <jwt>`，认证仍成功，两个接口都读到新名称。这提供了“完整 Authorization 原文参与键”的实际证据，而不只是不同用户自然得到不同结果。
3. 撤销该功能授权后，再请求两个已缓存接口，均返回空体 403 并带 NewToken；匿名请求均为空体 401，带认证挑战。说明缓存没有绕过认证或权限检查。
4. 上述拒绝请求之后读取 Redis，相关缓存键仍具有约 1.4～4 秒剩余 TTL；距本轮暖缓存约 57.5 秒，排除了因缓存已经过期才出现权限拒绝的解释。
5. 恢复授权后，在距暖缓存约 77.4 秒时再次使用原 Authorization，两个接口均返回更新后的名称和 NewToken，符合 60 秒过期回源。

命中判断采用业务字段值，不把首次对象序列化与缓存反序列化后的完整响应字节相等作为条件。续签头也不从响应缓存复用，而由本次鉴权管线生成。

源码入口：[客户端 User Feature](D:/ZWork/rust_engine/presentation/client_webapi/src/features/user.rs)、[Authorize Feature](D:/ZWork/rust_engine/presentation/client_webapi/src/features/authorize.rs)、[共享缓存](D:/ZWork/rust_engine/project_core/webapi_core/src/cache_data.rs)。两个 Feature 均将 60 秒、包含 Authorization 的缓存包装在权限中间件内。源配置见 [Client UserController](D:/ZWork/NetEngine/Presentation/Client.WebAPI/Controllers/UserController.cs)、[Client AuthorizeController](D:/ZWork/NetEngine/Presentation/Client.WebAPI/Controllers/AuthorizeController.cs) 和 [CacheDataFilter](D:/ZWork/NetEngine/ProjectCore/WebAPI.Core/Filters/CacheDataFilter.cs)。

## 权限路由同步

使用两个宿主已有的 `/Authorize/UpdateRoute`。没有新增路由清单输入接口，也没有修改宿主 Feature 注册。

### 事务回滚

隔离 Client.WebAPI 模块先只有一条有效的 `B2/Obsolete` 路由。临时添加 NOT VALID CHECK 约束，拒绝同步清单中的 `User/EditUserPhoneBySms` 写入。应用先处理旧路由删除和前面的新增，后续写入触发失败，接口返回生产全局错误 400。

读取数据库确认 Client.WebAPI 仍只有原来的 Obsolete 有效行，没有残留前面新增的路由，也没有残留删除标记。移除临时约束后正常同步成功。该约束只用于隔离运行故障注入，未改项目迁移或持久化模型，验收后数据库确认无残留约束。

### 正常同步、模块隔离和软删除重建

- 管理端、客户端正常同步均返回 200 空体；有效权限路由分别为 59 和 9。数量是权限同步清单，不是所有业务 HTTP 路由数量。
- 旧 Obsolete 行被软删除。初始化数据中另外两个名称为 `admin_webapi`、`client_webapi` 的模块保留原来的 57/26 条有效记录，未被当前 Admin.WebAPI/Client.WebAPI 同步修改。
- 给 Client.WebAPI 的 User/GetUser 旧记录绑定功能，再将该记录软删除；重新同步后得到不同 ID 的新有效记录，FunctionId 为 NULL；旧记录仍删除且保留旧绑定。没有复活旧授权关系。

### 空清单与并发的限制

[PermissionRouteService::sync_routes](D:/ZWork/rust_engine/application/application_service/src/permission_route/service.rs) 没有“空清单直接返回”的分支；空切片会在同一事务中软删除本模块所有有效路由。源码还保留读取时的 xmin，单次写入失败会阻止整批提交。源 [AuthorizeMaintenanceService](D:/ZWork/NetEngine/Application/Application.Service/AuthorizeMaintenanceService.cs) 也按模块维护路由并统一 SaveChanges。

现有两个宿主都固定注册非空清单，正式 UpdateRoute 不接收调用方清单。因此本轮不新增专用入口，不声称已实测空清单、指定 xmin 冲突时序或多个同步请求之间的全部交错。它们是保留的验证缺口，不是本轮确认的生产缺陷。

## 构建、检查与收尾

- 本轮容器内 `cargo build --locked --release -p admin_webapi -p client_webapi` 退出 0，耗时约 1 分 7 秒；用于验收的两个 API 均为本轮 Release 构建。
- `cargo fmt --all -- --check` 和已跟踪改动的 `git diff --check` 通过。生产代码没有再变化，沿用 B1 结束时同一工作区的 check、严格 Clippy 和 workspace build 结果，没有重复全量编译。
- 没有添加或运行测试，没有安装本机组件，没有重建发布包或运行镜像。
- 完成本轮验收的两个 Production 宿主均经 SIGTERM 正常退出，退出码 0。早期临时 Redis 配置地址/重复键问题已在容器临时配置中修正，没有归类为项目生产缺陷。
- 临时 PostgreSQL/Redis 容器和数据已删除，容器临时目录中的密钥、令牌、配置与日志已清理；原始临时日志不入库，结果摘要保存在本报告。仅原 dev 服务继续运行。未提交或推送，B1 的未提交改动保持原样。

本批有限范围到此结束；先审查和决定提交，再决定下一目标。不要因保留的空清单等限制自动重开本批，也不要把本报告解释为 B2 的全部可能运行场景已覆盖。
