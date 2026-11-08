# B8 微信小程序绑定识别：修复记录

日期：2026-11-08。本批针对 [阶段清单 S01/B8](D:/ZWork/rust_engine/docs/netengine-next-stage.md)，用户已明确同意修改 NetEngine 源项目。

## 基线与实际改动

- Rust 基线为 `acd147dccbe26dbb766b1479897c5e5cb1806e97` 加 B7 尚未提交的文档。本批没有改动 Rust 生产代码，B7 文档改动保留。
- NetEngine 基线为 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 加此前五处授权修改。本批新增修改仅为 [AuthorizeService.cs](D:/ZWork/NetEngine/Application/Application.Service/AuthorizeService.cs:87) 的一个查询条件，保留其他文件原有改动。
- `GetTokenByWeiXinMiniAppAsync` 已获取微信返回的 openId，但原查询把绑定的 OpenId 与应用 AppId 比较。改为与 `openId` 比较，仍同时限定 `AppName == "WeiXinMiniApp"` 和 `AppId == login.AppId`。
- 锁外查询和获锁后的复查共用 `userIdQuery`，这次修复同时覆盖两处。新增绑定原本就保存相同 openId，无需改变保存逻辑、接口或数据库结构。
- 修改后源文件 SHA-256：`54baf94c9fd04755b039da4311e5cdb51af73958da2306d6d8383c5f2df7a112`。

## 登录分支核对

| 分支 | NetEngine 与 Rust 的当前源码依据 | 本批证据 |
| --- | --- | --- |
| 微信身份获取 | 按 AppId 取得配置，调用 jscode2session，要求返回 openid 和 session_key | 读取源码；没有真实外部调用 |
| 已有用户 | 同时按应用类型、AppId、微信返回的 openId 查找绑定；找到后复用关联用户 | 源查询已修复；Rust 原本正确 |
| 锁内复查 | 未找到时使用 `GetTokenByWeiXinMiniAppCode` 加 openId 加锁，获锁后再次查询 | 双方路径已核对，没有运行并发登录 |
| 首次注册 | 创建用户和外部绑定；源一次 SaveChangesAsync 保存两项，Rust 同一事务保存两项并提交 | 源码确认事务边界，没有运行外部首登 |
| 软删除 | 源 DatabaseContext 应用生成的 DeleteTime 全局过滤；Rust 查询显式过滤绑定和关联用户的 DeleteTime | 核对模型/生成器和查询代码，没有对 EF 生成 SQL 作运行验证 |
| 签发 | 取得用户 ID 后，先保存 UserToken，再构建含 userId/tokenId 的 JWT | 核对调用链；不重复前序认证链验收 |

Rust 对应代码位于 [授权服务](D:/ZWork/rust_engine/application/application_service/src/authorize/service.rs:226)。`find_external_user` 保留应用类型、AppId、OpenId 及双侧软删除过滤；`create_external_user` 保留用户/绑定的同一事务保存。本批未发现这段链路需要新增 Rust 修改。

源软删除依据为 [DatabaseContext](D:/ZWork/NetEngine/Repository/DatabaseContext.cs:103) 调用 `ApplySoftDeleteFilters`，及 [SoftDeleteFilterGenerator](D:/ZWork/NetEngine/SourceGenerator/SourceGenerator.Core/EntityFrameworkCore/SoftDeleteFilterGenerator.cs) 生成的 `DeleteTime == null` 查询过滤。关联用户导航和首次保存等行为没有通过新建测试程序或临时接口验证。

## 容器编译结果

NetEngine 在已有 `mcr.microsoft.com/dotnet/sdk:latest` 镜像中执行：

```text
dotnet build Application/Application.Service/Application.Service.csproj --configuration Release --artifacts-path /tmp/b8-artifacts --nologo --verbosity minimal
```

构建成功，0 警告、0 错误，约 12 秒。构建读取只读挂载的 NetEngine 源码，沿用现有 `netengine-nuget-cache` 命名卷；输出在临时 SDK 容器内，未写入 Windows 的 bin/obj，也未安装本机 SDK。仅构建受影响应用项目及必要引用，不包含 Admin.App。临时构建容器使用 `--rm`，退出后已删除。

Rust 在原有 Docker dev 内执行并通过：

- `cargo check --locked -p application_service`
- `cargo fmt --all -- --check`

两仓库本次改动的空白检查通过。没有添加或运行测试，没有升级依赖，没有修改 stable/latest 约定。

## 运行限制与交付

没有可用的真实微信 AppId/AppSecret 和一次性登录 code，本批未读取现有业务库凭据，也没有调用微信、搭建模拟提供商或新增验证程序。因此登录成功、首登并发、软删除后的实际 EF 查询、令牌结果仍只有本批源码依据，不能宣称端到端登录已验收。

修复只影响后续绑定查找，不扫描、合并或删除可能已有的重复账号及绑定。若实际业务库存在历史重复数据，需单独明确保留与合并规则；本批不触及真实数据。

本批生产改动位于旁边的 NetEngine 仓库，rust_engine 仅更新阶段记录。两个仓库均未提交或推送，原有未提交改动保留，没有新增常驻容器或临时运行数据。
