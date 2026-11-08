# 部署配置与发布包

`deployment` 根据统一配置生成 Nginx、systemd、运行环境示例和云效流水线，覆盖 `admin_webapi`、`client_webapi`、`task_service`。生成器只写文件，不连接服务器、不安装服务，也不执行部署命令。

## 生成部署文件

修改 [deploysettings.json](../deployment/deploysettings.json)，在工作区根目录执行：

```powershell
docker compose exec -T dev cargo run --locked -p deployment
```

也可以运行 VS Code 的 `container deployment generate` 任务。生成器不接受启动参数，从当前目录或可执行文件目录向上查找包含配置和模板的 `deployment` 目录。

配置项沿用 NetEngine 的名称：

| 配置项 | 用途与约束 |
| --- | --- |
| `ProjectName` | 文件、服务和制品名称；英文字母、数字和中间连字符，生成名称转为小写 |
| `Host.AdminWebAPI` / `Host.ClientWebAPI` | `Domain`、`CertificateFile`、`CertificateKeyFile`，分别用于域名、证书和私钥 |
| `Port.AdminWebAPI` / `Port.ClientWebAPI` | 两个不同的 1–65535 端口；默认仅监听 `127.0.0.1` |
| `DeployRoot` | 非根目录的 Linux 绝对路径；路径段允许字母、数字、点、下划线和连字符，不允许 `.`、`..` 或空段 |
| `YunXiao.Codeup` | `Name`、`Endpoint`、`Branch`、`ServiceConnection`，用于代码源 |
| `YunXiao.MachineGroup` | 主机部署机器组标识 |

域名不包含协议、端口或路径。证书路径允许上述 Linux 路径字符，可使用绝对路径或相对 Nginx 配置根目录的路径。仓库配置使用 `example.com` 和示例云效标识，需要替换为实际项目参数；连接凭据放在服务器环境文件中。

生成器固定读取 [templates](../deployment/templates)，模板占位符为 `{{配置项}}`。云效参数按 YAML 字符串转义；新增模板变量需要同步生成器的映射。配置无效、缺少模板或存在未替换占位符时，旧结果保持不变。

全部文件成功写入临时目录后才替换 `generated`；替换失败会尝试恢复旧结果。生成期间持有目录锁，避免两个生成任务交错覆盖。**每次成功生成会整体替换旧目录，包括自行加入的文件。需要定制时修改模板。**

以 `demo` 为例，结果包含：

```text
deployment/generated/
├── nginx/
│   ├── nginx.conf
│   ├── demo-admin-webapi.conf
│   └── demo-client-webapi.conf
├── service/
│   ├── demo-admin-webapi.service
│   ├── demo-client-webapi.service
│   ├── demo-task-service.service
│   ├── demo-install-services.txt
│   └── demo-runtime.env.example
└── yunxiao/
    ├── demo-admin-webapi.yaml
    ├── demo-client-webapi.yaml
    └── demo-task-service.yaml
```

## 构建发布包

在本机已有的 Docker 开发容器中执行：

```powershell
docker compose exec -T dev sh deployment/build-release.sh admin_webapi
docker compose exec -T dev sh deployment/build-release.sh client_webapi
docker compose exec -T dev sh deployment/build-release.sh task_service
```

工具链跟随 `stable`，构建使用 `--locked --release` 和 `x86_64-unknown-linux-gnu` 目标。编译缓存保留在容器的 `CARGO_TARGET_DIR`，最终发布包输出到本机 `dist/<宿主>`，不需要在 Windows 安装 Rust。每次成功打包会整体替换该宿主的发布目录。

发布包中包含：

- 宿主可执行文件。
- 管理端的 `InitData/AuthorizeInitData.json`，供 Development 环境显式执行[权限初始化](authorization-initialization.md)。
- `presentation/<宿主>/appsettings.toml`，以及存在时的 `appsettings.Production.toml`。
- `build-info.txt`：构建工具链与构建环境的 glibc 信息。
- `runtime-libraries.txt`：二进制依赖的 Linux 动态库。

目标服务器需要 Linux x64、与构建产物兼容的 glibc 和所列动态库，以及 CA 证书、时区数据。服务器运行这些二进制不需要 Rust 或 Cargo。由于构建镜像使用 `rust:latest`，更新镜像后应重新确认目标系统的运行库兼容性；发布包中的记录便于核对。

三个 Dockerfile 的 `artifact` 阶段同样导出二进制和默认配置，`runtime` 阶段提供完整容器运行镜像。Docker 外部基础镜像统一使用 `latest`。

图形验证码通过 Skia 使用系统默认字体。Linux 运行环境需提供 Fontconfig、C++ 运行库和可用字体；Docker 运行镜像包含 `libfontconfig1`、`libstdc++6` 与 `fonts-dejavu-core`。自行部署发布包时也需准备这些运行依赖。开发镜像和云效构建模板包含 Clang、Ninja、Python 与 Fontconfig 开发文件，以便没有匹配预编译包时在容器内构建 Skia。

## 服务目录与配置

三个宿主分别解压到：

```text
<DeployRoot>/adminwebapi/
<DeployRoot>/clientwebapi/
<DeployRoot>/taskservice/
```

systemd 的工作目录指向对应目录，运行文件分别是 `admin_webapi`、`client_webapi` 和 `task_service`。随包配置保留源代码的相对路径，因此默认加载路径可直接使用。

服务设置 `APP_ENVIRONMENT=Production`，并依次读取以下可选文件，后者覆盖前者：

1. `<DeployRoot>/config/shared.env`
2. `<DeployRoot>/config/admin-webapi.env`、`client-webapi.env` 或 `task-service.env`

`*-runtime.env.example` 不会自动加载。按实际部署填写连接和密钥后建立对应 env 文件，并限制其读取权限。配置键使用 Rust 宿主现有的 `APP__节名__键名` 形式，例如：

```text
APP__DATABASE__CONNECTION="postgres://user:password@db/business"
APP__DATABASE__READ_CONNECTION="postgres://reader:password@db-reader/business"
APP__LOGGER__CONNECTION="postgres://writer:password@log-db/logs"
APP__LOGGER__READ_CONNECTION="postgres://reader:password@log-reader/logs"
```

连接中的特殊字符需要按 URL 规则编码。业务缓存和锁按模块隔离；同一业务的 API 与 Worker 使用相同目标及前缀。共享 ID 协调 Redis、日志库与维护锁、事件 Scope 分别按各组件的共享范围配置。日志读库不能指向业务读库。

外部完整配置可通过 `APP_CONFIG_PATH` 指向不带 `.toml` 后缀的路径。环境文件中的监听地址如果覆盖 `APP__HOST__LISTEN_ADDRESS`，需要同时调整 Nginx 上游端口。

## 首次部署

目标服务器准备 Nginx、systemd、`tar` 和二进制所需运行库。建立部署目录、放置发布包与证书，再将三份服务配置放到 `/etc/systemd/system`。服务模板默认使用 root，与参考项目一致；使用其他账号时，同步设置运行目录、日志及上传目录的访问权限。

将 API 配置放到 Nginx 的 `conf.d` 目录。`nginx.conf` 是主配置参考，包含 API 代理依赖的 `$connection_upgrade`；已有主配置时合并所需内容。检查配置后再重载 Nginx：

```sh
sudo nginx -t
sudo systemctl reload nginx
```

`*-install-services.txt` 提供 `daemon-reload`、启用和启动三个服务的命令。执行前确认发布包和运行配置已就绪。服务停止信号为 SIGINT，45 秒停止期限覆盖应用的 30 秒排空预算。

数据库结构仍通过 `repository_tool` 按 business、log、event-bus 分别迁移；部署生成器和流水线均不自动迁移数据库。

## 云效流水线

三份 YAML 分别用于创建三个宿主的流水线。构建任务在 Linux amd64 的 `rust:latest` 容器中准备构建依赖并调用 `build-release.sh`，上传 `dist/<宿主>`。代码源、服务连接及机器组由统一配置生成。

部署任务停止对应服务，将云效下载的制品解压到宿主目录，设置二进制执行权限后重新启动。现有 `Logs`、上传目录和外部 env 文件保留，不递归修改数据文件的权限。随包默认配置会更新，环境差异应放在外部配置中。首次部署前需要先安装 systemd 服务文件；流水线不会创建服务定义或安装运行库。
