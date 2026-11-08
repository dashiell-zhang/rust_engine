# 数据库工具

`repository_tool` 提供模型快照、增量迁移生成、迁移执行、迁移 SQL 导出和 JSON 索引差异命令。它不启动 WebAPI、任务、日志、事件或 ID 后台服务；无参数运行只显示帮助，不读取配置或连接数据库。

所有命令均在 Docker 开发容器中执行：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool
```

## 配置与目标

配置源文件是 [repository_tool/appsettings.toml](../repository_tool/appsettings.toml)。构建时复制为工具可执行文件旁的 `repository_tool.appsettings.toml`，默认按可执行文件位置加载，不受终端工作目录影响。修改源配置后重新构建；发布工具时将二进制和这份配置一并复制。

三个写模型分别选择自己的连接，没有连接时直接报错，不回退其他目标：

| `--target` | 工具配置字段 | 模型所有权 |
| --- | --- | --- |
| `business` | `database.connection` | 业务实体、队列与任务配置 |
| `log` | `database.log_connection` | 日志父表与分区 |
| `event-bus` | `database.event_bus_connection` | 事件消息和位置状态 |

目标名称不区分大小写，也接受 `EventBus`。执行命令必须通过 `--target` 明确选择目标。

工具只读取可执行文件旁的 `repository_tool.appsettings.toml`，不接受 `--config-path`、`--database-url` 或进程环境覆盖，也不加载环境专用配置和 WebAPI 配置。需要切换数据库时修改工具配置中的对应连接。

## 生成增量迁移

修改当前实体或组件的索引、默认值、分区声明后，先构建工具，再离线生成所选模型的迁移：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-add AddOrderRemark --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-add AddLogIndex --target log
```

命令比较当前编译模型与最后迁移保存的模型，不读取数据库结构，也不执行 DDL。名称只能包含字母、数字和下划线，不能与该模型已有迁移重名；版本号自动生成。同一模型的生成过程使用目录锁，生成后必须重新编译才能继续生成下一项迁移，旧二进制不会覆盖尚未编译的登记文件。

默认目录为业务 `repository_tool/src/migrator/generated`，日志和事件分别为所属组件的 `src/migrations/generated`。`baseline.json` 是对应旧迁移链的已核验基线；每个新版本目录包含 `up.json`、`down.json`、`model.json` 和 `migration.rs`，并由 `index.rs` 登记。它们一起提交并在编译时嵌入工具，发布运行无需读取源码目录。`--output-dir` 可指定其他目录，但该目录必须保有当前已登记的生成迁移；自定义输出不会自动修改默认目录的登记。

生成后审查 Up/Down，尤其是删除、类型变化、非空约束和唯一索引；必要的数据填充或自定义 SQL 应在执行前写入该新迁移。重命名目前表现为删除旧对象、新建新对象，需手工改成所需的重命名操作；Down 重建结构不能恢复已删除的数据。确认后重新编译，再单独执行 `migrate`。不要改写已应用的迁移或历史模型快照。

普通索引的删除或修改不会触发外键重建，因为外键不能依赖非唯一索引。涉及唯一索引、表、列类型或主键等可能影响关系的变化仍按保守顺序处理，生成结果需要审查；不能将普通索引的规则套用于唯一约束。

当前模型包含表、列类型、可空性、默认值、主键、命名外键、索引、注释及分区元数据；`xmin` 不作为普通列。当前业务的显式索引归属 `repository::schema`，队列索引归属 `queue_task_database::schema`，工具只做模型装配。已有分区表只允许调整影响未来分区的周期，不通过普通迁移切换分区或修改分区键。原生枚举、生成列、额外列约束及序列变更等尚未完整建模的情形会报错，需要补充模型支持及显式迁移，不能认为与 EF Core 迁移生成能力完全等同。

查看当前模型或检查是否还有未生成的变化：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- model-snapshot --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-pending --target business
```

`model-snapshot` 只输出当前声明，不覆盖历史快照；加 `--sql` 可审查当前模型的完整建库 SQL，该输出不含迁移历史，不能替代已有库的增量迁移。`migration-pending` 在有变化时以非零状态退出。默认 `migrate` 也拒绝尚未生成迁移的模型变化；显式 `--to` 仍可操作已登记的旧版本。这些设计时命令均读取工具配置但不连接数据库。

## 执行迁移

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target event-bus
```

使用 Redis 事件后端时可以省略事件数据库迁移。即使目标连接相同，也必须分别迁移所需模型。业务、日志、事件历史表分别是 `__EFMigrationsHistory_Business`、`__EFMigrationsHistory_Log`、`__EFMigrationsHistory_EventBus`，均固定在 `public` schema；迁移在事务和数据库锁中执行。历史使用 `MigrationId`（最长 150 字符，主键）和 `ProductVersion`（最长 32 字符）记录，Rust 工具写入自身版本。

重复运行会读取所选模型的历史版本，只执行尚未应用的迁移。业务迁移 `202611070002` 将用户角色组合索引调整为普通索引，保留现有关系记录；每条用户角色关联仍由自身 ID 标识。

`migrate --to` 可将所选模型迁移到指定版本，接受版本号、唯一名称或完整 MigrationId；省略时升级到最新。目标早于当前版本时，按逆序执行 Down 并删除对应历史记录；目标版本自身保留。`--to 0` 撤销全部已应用的迁移，会删除该模型的表及数据：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target business --to 202606240001
docker compose exec -T dev cargo run --locked -p repository_tool -- migrate --target log --to 0
```

执行前应审查目标版本及 Down 定义，并备份需要保留的数据。回退后可再次执行不带 `--to` 的命令升级；重新建表不会恢复已删除的数据。回退只操作所选模型，保留历史表及共享的 `citext` 扩展；日志父表的已附加分区随父表一起删除。外部依赖阻止删除时整次迁移失败回滚，不使用 `CASCADE` 删除其他模型对象。

早期已登记迁移使用独立的固定 SQL 定义：业务位于 `repository_tool/src/migrator/snapshots`，日志和事件分别位于所属组件的 `src/migrations`。后续生成迁移保存在各自 `generated` 目录。修改当前实体、索引或分区声明不会重写历史迁移；模型变化需要新增迁移并登记版本。日志迁移保存当时的父表和分区元数据，初始子分区仍按 SQL 生成时刻计算。

每个迁移的撤销语句保存在对应的 `.down.rs` 文件中。现有业务链的 Down 以已固定的前置版本为准：多数整理步骤的前置定义已经包含相同结构，因此只撤销历史记录；`202611070001` 撤销其新增的有效任务定义唯一索引。它们不能恢复固定迁移定义之前未保存的旧结构，也不能还原升级过程中已改写或删除的数据。已有数据库必须与这份迁移链相符，不能仅凭相同版本号假定任意旧库可以还原。

业务迁移 `202611070003` 将用户姓名改为 `citext`，保留原始文本并采用数据库的大小写比较规则；目标 PostgreSQL 需提供 `citext` 扩展，迁移负责启用。用户名及外部账户绑定不施加唯一约束，管理端保存用户时仍按应用服务规则检查用户名。用户的手机号、邮箱、时间和审计外键索引同时补齐。已有库需执行此迁移后再运行使用新实体定义的宿主。

业务迁移 `202611070004` 按当前模型调整角色、功能及授权、路由、LLM 应用、产品和订单索引；应用服务的查重规则仍然生效。用户与资料采用全表一对一关系，`UserInfo.UserId` 的唯一约束包含软删除记录。旧库若存在同一用户的多条资料，本次迁移会失败并回滚，保留原始数据和历史；需要先按业务归属处理重复资料，再重新执行迁移。工具不会自动删除或合并资料。

业务迁移 `20261107105933_align_stored_file_table` 将文件记录表 `File` 重命名为与 NetEngine 一致的 `StoredFile`，同步重命名主键、外键及显式索引。Up/Down 均只重命名对象，保留文件记录与 xmin，不移动或删除物理文件。已有 Rust 数据库需先执行该迁移再启动新版宿主；数据库中若已存在同名目标表，迁移会失败并回滚，不自动合并两张表。旧迁移和历史快照保持原样，空库由完整迁移链建立最终结构。

日志模型创建分区父表和当前周期的初始分区。后续分区维护由配置了数据库日志的运行时宿主负责。工具不会把既有普通表自动改成分区表，也不会搬迁历史数据。

迁移会修改选定数据库。初始迁移要求对应模型表不存在，不会跳过已存在的表并将其登记为迁移成功。已有库应使用与其历史相符的迁移链；工具不自动重命名旧历史表，也不把 EF Core 迁移文件或其他迁移链视为当前 Rust 迁移已执行。

## 迁移 SQL 导出

`migration-script` 读取工具配置并离线构建 SQL，不打开数据库连接。默认导出所选模型从空库到最新迁移的全部步骤，省略 `--output` 时输出到标准输出：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --output /workspace/dist/business-migrations.sql
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target log --idempotent --output /workspace/dist/log-migrations.sql
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target event-bus --idempotent
```

输出路径为容器内路径，缺少的父目录会自动创建；`/workspace` 对应当前工作区。输出文件使用 UTF-8，成功导出会覆盖同名文件。脚本生成失败时不写入文件。

`--from` 表示目标数据库已经应用到的迁移；`--to` 表示执行后保留到的迁移。升级应用起点之后、终点及之前的步骤；回退按逆序撤销起点及之前、终点之后的步骤，保留终点。两者可使用版本号、唯一名称或完整 MigrationId；起点默认 `0`，终点默认最新。相同起止位置生成空脚本。示例只导出初始业务迁移之后的步骤：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --from 202604050001 --to 202611070002
```

反向区间导出 Down SQL，例如从最新业务版本回退到 LLM 整理版本：

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- migration-script --target business --from 202611070004 --to 202606240001 --idempotent --output /workspace/dist/business-rollback.sql
```

普通脚本用于明确的起始版本；`--idempotent` 通过当前模型历史表逐个跳过已应用的升级步骤或已撤销的回退步骤。幂等只检查脚本区间内的历史，不检测人工结构变化，也不会撤销 `--from` 之后的版本。脚本在一个事务中执行，并与 `migrate` 使用相同的数据库锁；执行失败时回滚该脚本内的结构及历史记录。SQL 输出本身不会执行迁移，应审查后在选定数据库单独执行。日志初始分区按脚本生成时的周期确定，执行后由宿主继续维护未来分区。

导出和直接迁移使用同一份固定 Up/Down 定义。此命令不重新比较当前模型；生成增量迁移使用 `migration-add`。

## JSON 索引差异

```powershell
docker compose exec -T dev cargo run --locked -p repository_tool -- json-index --target business
docker compose exec -T dev cargo run --locked -p repository_tool -- json-index --target log
docker compose exec -T dev cargo run --locked -p repository_tool -- json-index --target event-bus
```

命令只读取 PostgreSQL 系统目录，并将差异 SQL 输出到标准输出。目标说明写入标准错误，便于重定向保存 SQL。目录读取使用固定连接上的只读事务；命令不会执行生成的 `CREATE INDEX` 或 `DROP INDEX`。

比较范围同时受两项限制：

- 表必须属于当前选定模型。业务迁移与索引比较共用同一实体清单，日志和事件组件分别提供自己拥有的表。
- 既有索引名称必须符合工具保留形式 `jsonbIX_` 加 32–48 位十六进制字符。其他索引及模型外表不作为删除候选。

新索引使用定义的 SHA-256 前 40 位十六进制作为名称后缀。索引定义包含实际 schema、表名、列或 JSON 标量表达式、排序方向和唯一性。不要手工修改工具拥有的索引或复用其保留名称；调整声明后重新生成并审查 SQL，再由部署方执行。

未显式映射 schema 的实体按所选连接的 `search_path` 解析，缺少模型表时整次命令报错，不输出部分脚本。当前业务模型没有 JSON 索引声明，因此在没有旧工具索引的库上输出为空。

## 模型声明

当前模型将主键名称记录为独立快照字段，默认使用 `PK_表名`；SeaORM 默认外键名统一为 `FK_表名_引用表名_列名`，显式自定义名称保持不变。旧快照没有主键名称字段时仍按原有 `表名_pkey` 解析，不改写历史快照。任务配置有效定义索引使用源模型的 `IX_TaskSetting_Category_Name` 名称。

主键字段不变且仅名称变化时生成 `RENAME CONSTRAINT`。外键和索引只在可识别的声明 SQL 除名称外完全一致、旧新定义能够唯一配对且没有同类名称占用时识别为改名；多行索引声明同样支持。纯索引改名不会触发外键重建。结构变化、歧义或未知 SQL 仍采用原有差异处理，生成结果需要审查。

SeaORM 实体在 `DeriveEntityModel` 之前添加 `#[repository_core::model_documentation]`，统一将结构体和字段的 `///` 文档转换为数据库表、列注释。显式 `#[sea_orm(comment = "...")]` 优先；没有文档时使用映射表名或列名，系统列 `xmin` 和 `ignore` 字段不参与。保留多行文档的换行，SQL 引号由共享迁移生成器处理。事件存储使用显式关系模型，其注释与列声明一起保存在所属组件。

文档更新后也需要运行 `migration-add`：注释进入模型快照，生成独立的 `COMMENT ON TABLE/COLUMN` 操作，Down 恢复旧注释。只更新注释不会重建表、外键或索引；未登记迁移时，`migration-pending` 同样报告模型变化。这里保留 Rust 实体已有的中文文档，不要求与 .NET XML 文档逐字相同。

公共实现位于 [repository_core::json_index](../infrastructure/repository_core/src/json_index.rs)。实体所属 crate 实现 `JsonIndexedEntity::json_index_model()`，通过 `JsonIndexTable::from_entity` 取得实际 SeaORM 表名和列类型，再补充声明。没有 JSON 索引的实体可使用 `impl_json_index_model!`。

`JsonIndexDefinition` 包含字段路径、逐字段排序、全部降序和唯一性；不允许空字段集合或重复字段。`IndexProperty` 将声明中的根属性映射到数据库列，复杂属性通过 `JsonMember` 声明属性名称、实际 JSON 序列化名称及成员类型。成员名称应与序列化模型一致，不能把 Rust 属性名直接当作经过重命名后的 JSON 键。

标量支持字符串、16/32/64 位整数、无符号大整数、十进制、浮点、布尔和 UUID。无符号 16/32 位值分别使用 `Integer`/`BigInt`，无符号 64 位值使用 `UnsignedBigInt`；有转换器时声明实际保存类型。集合路径、对象叶子和不支持的类型会报错，复杂属性必须映射到真实 JSON 列。

分区模型需要携带 `partition_key`，唯一索引必须包含映射到该分区键的普通列。日志组件复用其 `PartitionedModel` 声明取得分区键。索引目录查询不需要加载写入分区实体所需的快照。
