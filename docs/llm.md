# LLM 模型与应用调用

LLM 模块将模型连接、提示词模板和调用用例分开。管理端维护模型与应用，业务代码通过应用 Code 发起普通或流式对话，无需在处理器中判断具体协议。

## 准备配置

1. 在管理端模型管理中填写协议、模型名、服务地址和接口密钥，并启用模型。
2. 创建 LLM 应用，设置 Code、选择模型并填写提示词模板。
3. 在宿主中启用所需能力，注册客户端工厂和模型配置解析器，见[LLM 资源装配](database-modules.md#llm-依赖替换)。

模型和应用管理使用业务写库，会话列表使用只读连接。编辑模型时密钥为空表示保留原值。具体 HTTP 路径和 DTO 可在管理端 Swagger 的 LLM 分组查看。

## 普通调用

以下片段位于已注册 LLM 调用能力的 WebAPI 处理器中，`summary` 是事先创建的应用 Code，`content` 是模板参数名：

```rust
use application_model::llm::parameters::LlmParameters;
use webapi_support::WebApiApplicationExt;

let mut parameters = LlmParameters::default();
parameters.insert("content", Some("需要总结的内容".to_owned()));

let service = app_state.services().llm_invoke()?;
let answer = service.chat_content("summary", parameters, None).await?;
```

`chat_content` 返回可空文本；需要完整候选和用量时使用 `chat`。第三个参数为可空操作人 ID，正式调用可用于记录会话。

参数默认区分大小写。需要忽略大小写时使用 `LlmParameters::new(LlmParameterComparison::OrdinalIgnoreCase)`。值可以为 null 或空字符串，缺失必填占位符会返回错误。

## 流式调用与取消

```rust
use futures_util::StreamExt;
use common::cancellation::CancellationToken;

let cancellation = CancellationToken::new();
let mut stream = service.chat_stream_with_cancellation(
    "summary",
    parameters,
    None,
    cancellation.clone(),
).await?;

while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    // 将 chunk 交给调用方或响应流。
}
```

示例中的 `parameters` 为新建的参数集合。流在消费时读取配置并执行调用，调用方应持续读取或显式取消；不应在后台脱离请求继续消费。

需要中止时对令牌副本调用 `cancel()`。取消覆盖配置读取、提供商调用和会话保存。普通调用也有 `chat_with_cancellation`、`chat_content_with_cancellation`。

正式流式调用仅在完整消费、收到非空结束原因后保存会话，并聚合第一个候选。提前丢弃、取消或只收到结束标记不保证保存。管理端试用不保存会话。

## 扩展与使用限制

- `extra_body` 传入温度、输出数量等协议参数，与基础字段重名会报错。
- `LlmClientFactory` 可替换协议客户端；`LlmModelConfigResolver` 可替换配置来源，无需修改 HTTP 处理器。
- 默认配置解析器读取业务写库，不缓存模型配置。
- 流式请求的响应头期限为 120 秒，后续正文随流消费与释放结束。
- 公共响应保留模型、候选、结束原因和可空用量；提供商是否返回某字段由具体协议决定。

服务契约见[LlmInvokeService](../application/application_interface/src/services/llm_invoke.rs)。
