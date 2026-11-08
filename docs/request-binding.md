# WebAPI 请求绑定

共享提取器负责将 HTTP 参数转换为模型并校验，只有绑定成功才调用业务服务。管理端和客户端使用相同规则，无需在每个处理器中重复解析 JSON 或查询字符串。

## JSON 请求

模型派生 `Deserialize` 和 `JsonModel`，处理器使用 `webapi_core::extract::Json<T>`。以下示例声明一个必填名称：

```rust
use common::json_helper::input::JsonModel;
use serde::Deserialize;

#[derive(Deserialize, JsonModel)]
#[serde(rename_all = "camelCase")]
#[json(type_name = "Example.CreateItem")]
pub struct CreateItem {
    #[json(required = "名称不能为空")]
    pub name: String,
}
```

端点使用 `#[webapi_core::webapi_endpoint(..., json_body = "item")]` 声明正文参数名，再通过 `Json(item): Json<CreateItem>` 接收正文。完整注册方式见[宿主扩展](host-extensions.md)。

```json
{"name":"示例商品"}
```

常用模型声明：

| 声明 | 用途 |
| --- | --- |
| `#[json(required = "提示")]` | 自定义必填提示 |
| `#[json(min_length = 1, min_length_message = "至少选择一项")]` | 集合最小长度 |
| `#[json(nested)]` | 验证实现 JsonModel 的嵌套对象或对象集合 |
| `#[serde(default)]` | 字段省略时使用默认值 |
| `#[json(value)]` | 由字段转换器处理字符串承载的 ID、日期等协议值 |
| `#[json(value, scalar_type = "System.DateTimeOffset")]` | 为自定义标量转换声明诊断类型，需配合 `deserialize_with` |
| `#[json(array)]` | 将集合的诊断类型声明为数组 |

模型支持具名、非泛型字段，不支持 `flatten`。字段名匹配不区分大小写，字典键和动态 JSON 内部属性保持原样。合法的重复模型字段取末值，但较早的类型错误不会被后面的合法值掩盖。

## 查询参数

查询模型派生 `QueryModel`，通过 `webapi_core::extract::Query<T>` 接收：

```rust
use common::json_helper::input::QueryModel;
use serde::Deserialize;

#[derive(Deserialize, QueryModel)]
#[serde(rename_all = "camelCase")]
pub struct ItemQuery {
    pub id: i64,
    #[serde(default)]
    pub include_details: bool,
}
```

请求示例：`GET /Item/Get?id=123&includeDetails=true`。

标量参数重复时取第一个值；没有默认值的必需字段缺失时返回错误。使用 `#[query(validate = "函数路径")]` 添加字段验证，函数接收字段引用并返回 `Result<(), InputError>`。Query 会汇总不同字段的错误，错误结果不会作为部分模型传入业务。

分页模型使用 `PageQuery`，页码、每页数量和总数使用 `i32`，每页数量为 1～100。声明示例见[栏目分页模型](../application/application_model/src/article/models.rs)。OpenAPI 查询模型同时声明 `#[into_params(parameter_in = Query)]`。

## 可空值、数字和日期

显式使用以下转换器，可将空字符串、纯空白和 null 处理为 `None`：

```rust
#[serde(default, deserialize_with = "common::json_helper::deserialize_optional_json_scalar")]
pub remark: Option<String>,

#[serde(default, deserialize_with = "common::json_helper::deserialize_optional_query_scalar")]
pub is_enable: Option<bool>,
```

以上是模型字段片段，分别用于 JSON 与 Query。数组、对象、动态 JSON 和响应模型不使用这些标量转换器。

- 数值输入检查目标类型范围，支持声明允许的数字字符串；普通数值字符串不裁剪空白，i64 使用独立转换规则。
- 非可空 JSON 布尔值使用 `true` 或 `false`，不能用字符串代替。Query 接受大小写不同的布尔文本。
- 字符串字段不自动把数字或布尔值转换为文本；非空字符串保留首尾空白。
- JSON 日期使用 ISO 格式，例如 `2026-11-08T09:30:00+08:00`。Query 中的 `+` 应编码为 `%2B`，避免被解析为空格。
- 无偏移日期按宿主本地时区解释。需要明确时间点时应发送偏移或 `Z`。
- Guid 查询字段可使用 `common::json_helper::input::deserialize_query_guid`；支持 N/D/B/P/X 格式。

## 正文与错误响应

JSON 接受 `application/json`、`text/json` 和 `application/*+json`，默认 UTF-8，也支持已声明的 UTF-16 小端字符集。不支持的媒体类型或编码返回 415 ProblemDetails。

空正文和根 null 不会变成默认业务对象；空对象 `{}` 仍执行字段验证。对象和数组最多嵌套 64 层，类型、范围或格式错误返回 400。错误文本用于诊断，调用方应先依据状态码判断失败。

需要保留原文验签时使用 `JsonWithBody`，不应重新序列化模型代替原始正文。普通文件上传使用 `uploaded_file::FormFile`，详见[文件接口](webapi-files.md)。

处理顺序为认证、续签、权限、参数绑定，再进入签名、缓存、请求队列等 Action 过滤器。参数错误时不会调用业务函数。

## 响应格式

处理器使用共享 `Json` 返回对象：

- i64（包括 ID、可空值和集合元素）输出为 JSON 字符串；i32、u64 和动态文档数字保持数字。
- 明确标记的偏移日期输出 ISO 格式，最多七位小数秒，零偏移写为 `+00:00`。
- 字典键与普通字符串保持原样，字段按模型声明顺序输出。
- 非有限浮点数不能作为普通 JSON 数字输出；对象和数组深度同样限制为 64。

异步数组可使用 `webapi_core::json_stream::JsonStream`，按项写出，不进入对象响应缓存。对象默认输出 JSON，字符串默认输出纯文本；文本格式化支持 UTF-8 和 UTF-16 小端。

响应缓存与 ETag 使用内部对象序列化，因此缓存命中的日期文本可能为 `yyyy/MM/dd HH:mm:ss zzz`，与首次响应的 ISO 文本不同。文件、null 和异步数组不参与对象缓存与 ETag。
