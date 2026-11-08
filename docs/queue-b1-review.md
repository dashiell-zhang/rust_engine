# B1 队列事务与恢复：结果记录

日期：2026-11-08。范围来自 [阶段清单 B1](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。本批结束，不进入 B2/B3；以下限制保留，不能概括为四项全部运行验收通过。

## 基线与环境

- Rust 基线：`0ce43aed8b76cd130ed713819e8336706f0335a8`。本批生产改动仅为 `project_core/task_service_core/src/runner.rs` 与 `infrastructure/queue_task_database/src/store.rs`，尚未提交。
- 最终工作区文件 SHA-256：`store.rs` 为 `834e8b52ded92d206f0b66009beb302eb021659686f61ab7551825a9a28ab3c4`，`runner.rs` 为 `6203e5ff0b46822a32151dc3c75b85df9f5a905b51c334d45a7186b70f79177c`。摘要按本地字节记录，换行转换会改变摘要。
- NetEngine：`23a7d0002938d0308c975dc48c04ac66aae0dda8` 加阶段清单记录的五处授权修改。本批只读源代码，没有启动 .NET 对照宿主，没有修改源项目。
- 使用现有 Docker dev，在独立临时 `rust-engine-b1-postgres` / `rust-engine-b1-redis` 中验收。PostgreSQL 实际为 18.6，镜像标签仍使用 latest；不访问原有停止中的 postgres/redis 容器或真实业务库。
- 使用正式 repository_tool 初始化隔离 business 模型；不在宿主启动时迁移。任务使用正式 Release 二进制，两实例共享 Redis 锁和 ID 租约；配置队列锁租约为一秒，演示任务并发数为二。
- 管理端使用本批修改前已编译的 Debug 二进制，仅用于 InitData、GetToken 和 RetryQueueTask 正式入口。重试实现本批未改动；不以此证明 Release 签名或 B2 的鉴权链。
- 隔离配置、日志与登录令牌只放在开发容器 `/tmp/rust-engine-b1`。直接 SQL 仅准备隔离场景和读取状态；没有新增测试代码、测试依赖、辅助服务或模拟外部提供商。

## 修复 Q01：短租约在首次续期前可能过期

[runner.rs](D:/ZWork/rust_engine/project_core/task_service_core/src/runner.rs) 允许配置最短一秒的有效租约，旧续租间隔为整秒的 `lease / 3` 再限制到 5～30 秒。因此一至四秒租约会在首次续期之前到期；五秒租约也没有提前续期余量。

改为 Duration 精度的三分之一，最多 30 秒：一秒约 333 毫秒，三秒一秒，默认 300 秒仍为 30 秒。没有改变领取、租约所有权或重试退避规则。

这是根据代码与配置契约确认的缺陷。NetEngine 使用固定默认租期，不能直接把源方法的五秒下限套用于 Rust 可配置短租约。本批正式实例用一秒租约完成短任务；现有演示处理器没有长等待入口，未声称已经实测持续多轮续期或网络延迟边界。

## 修复 Q02：兄弟任务完成时的父记录锁升级死锁

### 实际发现

修复前运行两个实例，父任务 200 已自身成功，20 个演示子任务 201～220 同时具备执行条件。结果 19 个成功，子任务 208 因 PostgreSQL `deadlock detected` 回到 Pending，Count=1；父子树没有完成，回调未生成。

数据库在 00:50:14 UTC 记录两个完成事务互相等待，等待语句均为父记录的 `SELECT ... FOR UPDATE`；记录定位到父任务 200。后续为了读取真实 SQL，使用另一个父任务和 20 个子任务进行一次有限诊断，该轮没有复现死锁，说明故障依赖竞争时序，不能用单次通过排除它。

### 原因与改动

Rust 在同一事务中先更新子任务的成功状态，再更新子树完成时间，然后锁父记录。PostgreSQL 对本事务已产生的新行版本再次更新时，可能重新执行外键检查，即使外键值没有改变；检查会持有父记录的 KEY SHARE。兄弟事务分别取得该锁后再升级为 FOR UPDATE，就可能互相等待。

该判断结合了实际 SQL/死锁日志和 PostgreSQL 的 [外键触发器实现](https://raw.githubusercontent.com/postgres/postgres/REL_18_STABLE/src/backend/utils/adt/ri_triggers.c)（`RI_FKey_fk_upd_check_required` 的当前事务行版本分支）。锁兼容性依据 [PostgreSQL 行锁说明](https://www.postgresql.org/docs/18/explicit-locking.html#LOCKING-ROWS)：NO KEY UPDATE 与 KEY SHARE 兼容，但仍与其他状态写入锁互斥。

[store.rs](D:/ZWork/rust_engine/infrastructure/queue_task_database/src/store.rs) 的完成路径 `lock_task` 改为 `FOR NO KEY UPDATE`。该路径不修改主键，仍按父链取行锁，成功状态、父子树状态和回调仍在同一事务提交；没有改为进程内锁，没有移除 xmin 条件，也没有添加失败后的无限重试。

NetEngine 在 [QueueTaskStore.cs](D:/ZWork/NetEngine/Infrastructure/QueueTask.Database/QueueTaskStore.cs) 中先锁父链，再统一 SaveChanges；不能因双方都叫“行锁”就认定不同写入次序的锁行为一致。本批未推断源项目也存在同样死锁。

### 修复后有限复核

| 场景 | 观察结果 |
| --- | --- |
| 父任务 600，40 个直接子任务 601～640 | 40 个全部 Succeeded，Count 均为 1，实际由两个 Worker 执行 |
| 根任务 700、中间任务 701、20 个孙任务 702～721 | 20 个全部 Succeeded，Count 均为 1，两个 Worker 均参与；700/701 子树完成时间均已写入 |
| 两个父链的回调 | `b1-fixed-parent`、`b1-fixed-root` 各一条，回调自身各执行一次 |
| 原失败子任务 208 | 正式 RetryQueueTask 返回 200，重执行后父任务 200 子树完成，`b1-parent` 回调恰好一条 |
| 数据库死锁计数 | 修复前为 1，修复后上述场景结束仍为 1，无新增死锁 |
| 实际执行 SQL | 数据库日志确认完成路径使用 `FOR NO KEY UPDATE` |

这些结果支持本次锁冲突修复，不构成任意任务图、任意数据库隔离级别或全部并发交错均无死锁的证明。

## 四项验收结果与限制

| B1 项目 | 本批结果 | 未覆盖部分 |
| --- | --- | --- |
| 1. 并发子任务完成、回调与事务 | 发现并修复 Q02；双实例直接/多级父链及回调数量复核通过；错误完成事务回滚有实际证据 | 未穷举所有并发交错；未运行源端同场景对照 |
| 2. 手动重试与 Worker 竞争 | 正式管理入口四状态结果正确；重试后由正式 Worker 执行成功；源码确认行锁和 xmin 保护 | 没有通过人为持锁制造“读取与提交之间”的指定交错，不能声称实际复现了全部重试/续租竞争 |
| 3. 长执行续租与所有权 | 修复 Q01；有效租约记录未被领取，过期旧 Worker 记录被新实例接管；源码核对所有权写入条件 | 持续多轮续期、旧处理器仍在运行时被接管后再尝试完成，缺少正式长任务入口，未实测 |
| 4. 停机与租约恢复 | 两实例 SIGTERM 正常退出码 0；到期任务在停止期间未领取，重启后执行；过期 Running 记录恢复通过 | 长任务超出 30 秒排空预算、强制取消后的服务器查询/副作用，未实测；未重试历史被审批拒绝的持锁停机方式 |

### 事务回滚的具体证据

隔离父记录 300 的回调名故意设置为空白，子记录 301 使用已有 SendSMS 演示处理器。业务处理器完成后，回调生产者拒绝无效名称，整个完成事务回滚；随后正常失败处理把子任务恢复 Pending。观察到子任务没有 SuccessTime/ChildSuccessTime，父任务也没有 ChildSuccessTime，且没有回调记录。

这是通过直接准备无效隔离数据触发的回滚场景，不是正常生产者允许创建的任务，也不是源端业务输入行为的对齐证明。处理器外部副作用不属于该数据库事务，仍需业务幂等。

### 管理重试与租约恢复的具体证据

| 隔离任务 | 调用/动作 | 结果 |
| --- | --- | --- |
| 101：Running，有效租约 | RetryQueueTask | 400，提示仍在执行；状态/次数/所有者保持，两个 Worker 也未领取 |
| 102：Running，过期租约，Count=2，未来 PlanTime | RetryQueueTask | 200；回到 Pending，Count=0，所有者/租约/计划/首次和最近时间清空；随后执行一次成功 |
| 103：已成功 | RetryQueueTask | 400，提示已成功任务不允许重试 |
| 104：Failed，Count=3 | RetryQueueTask | 200；重置后执行一次成功 |
| 400：旧进程 Running、租约过期、Count=1 | 不调用手动重试，直接由运行实例扫描 | 新 Worker 接管，Count=2，Succeeded，子树完成 |
| 900：停机前写入、十秒后到期 | 停止两个 Worker，确认到期后再读取；然后重启一个 Worker | 停止期间 Pending、Count=0；重启后 Succeeded、Count=1 |

所有者保护的源码依据：`try_claim` 以读取到的 xmin 更新；`renew_lease`/`start_execution`/失败处理要求 Running 且 WorkerId 匹配；完成路径先锁记录，再核对 WorkerId 和 Running 状态。另一 Worker 接管后，旧 Worker 不能通过这些条件提交新的完成状态。租约时间到达本身不等于 WorkerId 已更换；不要把它解释为业务副作用获得“恰好一次”保证。

## 编译与检查

全部在 Docker dev 内完成。首次续租修复后做过检查；实际运行新发现死锁并修改 store 后，重新执行以下必要检查，最终全部退出 0：

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --lib --bins --locked -- -D warnings`
- `cargo build --workspace --locked`
- `cargo build --locked --release -p task_service`（最终增量构建约 35 秒）

没有添加或运行 `cargo test`，没有安装本机组件，没有改动依赖或数据库迁移定义。运行镜像和发布包没有重建，不能直接据本轮结果发布旧镜像。

## 收尾与下一步

本批仅修复上述两个生产问题并记录运行证据。B1 的有限执行阶段到此结束；未覆盖项保留在本文件，不自动扩展为新任务。先审查改动并决定阶段性提交，再决定下一批。B2/B3 未开始。

本批所有临时宿主均以 SIGTERM 正常退出（退出码 0）。两个临时基础设施容器及其临时数据已删除，开发容器内临时配置、令牌和日志已清理；仅原 dev 服务保持运行。运行结果摘要保存在本文件，原始临时日志未纳入仓库。上一阶段尚未提交的盘点文档保留，没有替用户提交或推送。
