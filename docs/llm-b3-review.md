# B3 LLM 读取深度：结果记录

日期：2026-11-08。范围来自 [阶段清单 B3](D:/ZWork/rust_engine/docs/netengine-next-stage.md)。D01 的生产代码修复与容器编译验收完成；没有可用的隔离提供商与模型配置，本批未执行提供商响应的运行验收，不将编译通过等同于协议完全一致。

## 基线与改动

- Rust 基线为 `0ce43aed8b76cd130ed713819e8336706f0335a8` 加 B1 尚未提交的生产改动及 B1/B2 文档。本批唯一生产改动为 [wire.rs](D:/ZWork/rust_engine/infrastructure/llm/src/wire.rs)。
- NetEngine 对照基线沿用 `23a7d0002938d0308c975dc48c04ac66aae0dda8` 及阶段清单记录的五处授权修改。本批未修改 NetEngine，未运行 .NET 宿主作同步对照。
- 三种源适配器使用 System.Text.Json 且未覆盖默认最大深度；默认有效上限为 64，来源及位置保留在阶段清单 D01。
- 本批 `wire.rs` 的 SHA-256：`5ac12695271d8a8448e205b78e3ee766b97f09aff5a62bc43ea3ed0ee90999d8`。
- 未改变 Cargo 依赖、锁文件、宿主装配或模型字段，未添加测试、验证程序、模拟提供商或运行入口。

## 读取规则与兼容性核对

深度通过 `DeserializeSeed` 随对象属性值和数组元素传递，在读取容器时检查，而非完整构造响应树后检查。根对象或数组为第一层，第 64 层容器可以包含标量，第 65 层对象或数组拒绝；空容器也适用。根标量不占容器层数，每次普通响应或 SSE 事件读取重新从零计数。即使 DTO 随后忽略某个字段，该字段的 JSON 仍经过同一读取边界。

对象继续用有序键值对保存，不预先合并重复属性；DTO 的字段转换与动态错误对象的原始属性处理保持分离。`null`、可空集合及集合元素的现有访问规则未改动。

当前工作区启用 serde_json 的 `arbitrary_precision`，数字可能通过内部 MapAccess 传递。因此读取器区分真实 JSON 属性与内部数字标记，数字恢复为 `serde_json::Number`，不增加容器深度，也不先转换成浮点数。真实属性恰好名为 `$serde_json::private::Number` 时仍按普通属性处理。

该区分沿用现有 [common 属性读取器](D:/ZWork/rust_engine/infrastructure/common/src/json_helper/input/json_key.rs) 的 newtype 边界方式。本批只读核对容器缓存中锁定的 serde_json 1.0.154：真实 MapKey 保留 newtype 边界，NumberFieldDeserializer 将内部标记交给 `visit_borrowed_str`。这属于源码依据，不是本批对所有数字词法或重复属性排列的运行证明。

## 三协议入口核对

| 协议 | 普通响应 | SSE 事件 |
| --- | --- | --- |
| Chat / OpenAI Compatible | [client.rs](D:/ZWork/rust_engine/infrastructure/llm/src/compatible/client.rs:77) 调用共享 `read_json` | [client.rs](D:/ZWork/rust_engine/infrastructure/llm/src/compatible/client.rs:124) 调用 `wire::from_slice` |
| OpenAI Responses | [responses.rs](D:/ZWork/rust_engine/infrastructure/llm/src/responses.rs:79) 调用共享 `read_json` | [responses.rs](D:/ZWork/rust_engine/infrastructure/llm/src/responses.rs:114) 调用 `wire::from_slice` |
| Anthropic Messages | [anthropic.rs](D:/ZWork/rust_engine/infrastructure/llm/src/anthropic.rs:83) 调用共享 `read_json` | [anthropic.rs](D:/ZWork/rust_engine/infrastructure/llm/src/anthropic.rs:120) 调用 `wire::from_slice` |

共享 [read_json](D:/ZWork/rust_engine/infrastructure/llm/src/http_response.rs:55) 最终也调用 `wire::from_slice`。由此核实六条读取路径覆盖本次限制；SSE 分帧、完成标记和错误映射没有新增分支。

## 验收结果与缺口

| 项目 | 本批证据 |
| --- | --- |
| 64 层限制及三协议共用入口 | 生产实现、调用链核对及容器编译通过；64/65 层实际输入未运行验证 |
| 重复属性、数字、动态错误、可空集合 | 核对读取器和当前依赖实现；没有执行组合边界响应验证 |
| 普通响应与流式完成/错误 | 未运行：本批没有可用的隔离提供商及对应数据库模型配置 |

现有正式入口为管理端 `/Operations/TestLlmApp`、客户端 `/api/Demo/TestLLM` 和 `/api/Demo/TestLLMStream`。应用层根据模型配置创建提供商客户端。没有为了获得边界输入新增模拟服务，也没有读取原有业务库中的凭据或调用真实付费模型。后续运行验收需要可用的隔离模型配置和能返回指定响应的提供商；该条件不阻塞本批按约定交付代码与限制记录。

## 容器检查

以下命令均在原有 Docker Compose `dev` 服务内执行并通过：

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --lib --bins --locked -- -D warnings`
- `cargo build --workspace --locked`
- `cargo check --locked -p llm`，补充核对 LLM 单独构建时不同的依赖特性组合。

首次检查会话结束后的输出未完整留存，收尾重新执行一次以取得可确认的退出结果；没有因文档修改继续重复构建。没有运行测试，本机未安装任何组件，stable/latest 约定不变。

本批没有启动临时服务、创建临时数据库或安装依赖，也没有提交或推送代码。B1/B2 的未提交改动保留。范围之外的对象克隆、异常文案和其他提供商细节没有扩入本批；本批交付不代表 NetEngine 全部功能已完全对齐。
