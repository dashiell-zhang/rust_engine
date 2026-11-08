# B6 Linux 发布包：结果记录

日期：2026-11-08。范围来自 [阶段清单 B6](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。当前三个 Linux 发布包完成限定运行验收，没有发现需要修改生产代码、发布脚本或模板的问题。发布包保留在 `dist`，该目录按现有规则不纳入 Git。

## 基线与对照

- Rust 基线为 `0ce43aed8b76cd130ed713819e8336706f0335a8` 加 B1 的队列改动、B3 的 LLM 读取器改动及前序文档。本批没有修改生产代码、依赖、默认配置或模板。
- NetEngine 基线沿用 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及阶段清单记录的五处授权修改；本批只读对照，没有修改或运行源宿主。
- 对照源 [部署生成器](D:/ZWork/NetEngine/Deployment/Program.cs)、[客户端服务模板](D:/ZWork/NetEngine/Deployment/Templates/service/Client.WebAPI.service.template) 及服务/代理/云效文件结构。Rust 保留文件生成与实际部署的职责分离、三个宿主目录、外部环境文件和正常停止信号；没有纳入 Admin.App。
- 不执行真实服务器命令、systemd 安装、Nginx 重载或云效发布。模板的这些部署行为只有本批源码/生成结果依据。

## 当前发布包

在 Docker dev 顺序执行正式脚本：

- `sh deployment/build-release.sh admin_webapi`
- `sh deployment/build-release.sh client_webapi`
- `sh deployment/build-release.sh task_service`

三次均成功完成 `cargo build --locked --release --target x86_64-unknown-linux-gnu` 并生成发布目录。构建耗时分别约 2 分 03 秒、27 秒、1 分 24 秒；这不是无缓存性能比较。

| 发布目录 | 必要文件核对 | 当前二进制 SHA-256 |
| --- | --- | --- |
| [admin_webapi](D:/ZWork/rust_engine/dist/admin_webapi) | 可执行文件、默认配置、InitData、ASP.NET Core 许可证、构建信息、运行库记录 | `d31071e82a9ddb626a03c3f815dafbaf953541f467259246426f98db91829a79` |
| [client_webapi](D:/ZWork/rust_engine/dist/client_webapi) | 可执行文件、默认配置、ASP.NET Core 许可证、构建信息、运行库记录 | `b99ad4f55219914d29bf6375de6a6a741e667607b56f57c7fc1e9d2aed11a00f` |
| [task_service](D:/ZWork/rust_engine/dist/task_service) | 可执行文件、默认配置、构建信息、运行库记录 | `883fb04784ed9a149c5c2e8487a5fbe9f1668ffe1746c04ab7112760ac905b71` |

三份配置分别位于 `presentation/<宿主>/appsettings.toml`。逐字节比较确认它们与对应源配置一致；管理端 `InitData/AuthorizeInitData.json` 及两个 API 包的许可证也与源文件一致。本轮没有随包 Production 覆盖文件。

随包 `build-info.txt` 记录本次实际使用的 stable 工具链为 rustc 1.99.1、LLVM 23.1.1，构建环境 glibc 2.41。这是构建快照，不是新增版本锁定；`rust-toolchain.toml` 和镜像标签约定没有修改。

## 独立运行环境

使用本机已有 `rust-engine-admin-alignment:latest` 作为运行环境，镜像身份为 `sha256:c96dd23c991893bec5a69735d83caa7ce8dc920293cd7bb9c61477431330fa1b`。实际系统为 Ubuntu 26.04.1 LTS、glibc 2.43；镜像已有 Fontconfig、C++ 运行库及 DejaVu 字体，未安装或挂入 Rust 工具链。

为每个宿主创建独立临时容器，将对应发布目录完整复制到 `/release`，将工作目录设为 `/release`，直接执行 `/release/<宿主>`。`/proc/1/exe` 与运行时 SHA-256 确认执行的是上表当前发布包二进制，未使用镜像内原有的旧管理端程序。

临时 PostgreSQL/Redis 分别为 `rust-engine-b6-postgres`、`rust-engine-b6-redis`，使用 latest 镜像，不映射主机端口。正式 repository_tool 初始化隔离 `b6` 数据库的三个模型；原有数据库容器未启动或访问。

三个宿主均设置 Production，通过临时 `APP__...` 环境变量覆盖数据库、Redis、JWT 和监听地址。没有设置 `APP_CONFIG_PATH`，实际使用随包默认路径。JWT 密钥由容器已有 OpenSSL 临时生成；没有真实凭据、外部提供商调用或真实消息发送。环境覆盖未写回发布包。

## 启动与停止结果

| 宿主 | 实际运行结果 |
| --- | --- |
| Admin WebAPI | 启动完成；`/healthz` 返回 200、纯文本 Healthy，并带禁止缓存响应头 |
| Client WebAPI | 启动完成；`/healthz` 返回 200、纯文本 Healthy，并带禁止缓存响应头 |
| TaskService | 输出“TaskService 启动完成”，保持运行；没有出现 Debug 人工选择提示 |

三个二进制在运行容器内执行 `ldd` 均无缺失库。两个 API 的 Fontconfig、FreeType、C++ 等依赖得到解析；任务宿主的 C++、libgcc、libm、libc 得到解析。该结论仅适用于本轮实际运行环境，不推广至全部 Linux 发行版。

随后向三个容器主进程发送模板约定的 SIGINT。三者均自行退出，退出码 0、OOM 标记为 false，没有强制终止。没有准备运行中的长任务或在途请求，因此本批不证明强制停机期限耗尽时的业务恢复能力。

## 部署生成与编译检查

正式 `cargo run --locked -p deployment` 在 dev 容器中通过，生成 11 份 Nginx、服务、环境示例和云效配置。生成结果与仓库相同，`git diff -- deployment/generated` 为空；没有只修改生成结果或执行生成文件中的远程部署命令。

本批三个发布包的 Release 构建及 `cargo fmt --all -- --check` 均通过。生产代码没有新增修改，沿用 B3 对相同源码的工作区 check/Clippy 结果，不因文档变动重复全量构建。未添加或运行测试，本机未安装组件。

## 清理与限制

三个运行宿主和两项临时基础设施容器已删除，临时工具、环境覆盖、密钥及运行日志已清理。递归清理临时目录曾被自动安全检查拦截，随后只删除本批明确创建的文件及空目录，清理完成；没有更换执行环境重试递归操作。三个 `dist` 发布包及其默认配置、构建/运行库记录保留，前序未提交改动保留，未提交或推送代码。

尚未验收三套 Dockerfile 当前 runtime/artifact 镜像的重新构建、systemd 实际安装、Nginx TLS 代理运行、云效上传/下载及远程部署、其他 Linux 系统兼容性，以及完整业务功能。这些仍归属 V09 的后续范围，不以本批启动成功替代。
