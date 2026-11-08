# RSA 请求字段解密

`webapi_core::rsa_decrypt` 提供按请求模型字段声明的 RSA 解密。仅将需要解密的接口参数改为 `RsaJson<T>` 或 `RsaQuery<T>`；普通 `Json<T>`、`Query<T>` 与响应序列化不受影响。当前业务接口没有启用该能力。

## 请求模型

模型使用 `source_generator_runtime::RsaDecrypt` 派生遍历，需解密的 `String` 或 `Option<String>` 字段添加 `#[rsa_encrypted]`。

```rust
use serde::Deserialize;
use common::json_helper::input::JsonModel;
use source_generator_runtime::RsaDecrypt;

#[derive(Deserialize, JsonModel, RsaDecrypt)]
#[json(type_name = "Example.Credential")]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub user_name: String,
    #[rsa_encrypted]
    pub password: Option<String>,
}

#[derive(Deserialize, JsonModel, RsaDecrypt)]
#[json(type_name = "Example.SubmitRequest")]
pub struct SubmitRequest {
    #[json(nested)]
    pub credentials: Vec<Credential>,
}
```

处理器参数使用 `RsaJson(request): RsaJson<SubmitRequest>`，并从 `webapi_core::rsa_decrypt` 导入 `RsaJson`。提取器先沿用共享 JSON 绑定规则，再解密已标记字段，成功后将模型交给业务服务。查询参数使用 `RsaQuery<T>`，模型另需派生 `QueryModel`，沿用共享 Query 绑定规则。

每层自定义嵌套模型都需要派生或实现 `RsaDecrypt`。支持 `Option`、`Box`、`Vec`、数组、切片，以及 `HashMap`、`BTreeMap` 的值；字典键不修改。未标记字符串、基本数值、日期、UUID、动态 JSON 值保持原样。无需遍历的自定义字段可显式添加 `#[rsa(skip)]`，不能同时标记为加密字段。

字段标记使用 Rust 字段身份，不受 Serde 重命名影响。普通字符串集合没有字段标记，因而不会被自动当作密文；需要解密集合成员时使用包含已标记字段的模型。

## 宿主配置

两端 WebAPI 的宿主设置支持可选 `[rsa]` 节：

```toml
[rsa]
private_key = "替换为实际 PKCS#1 或 PKCS#8 私钥"
```

私钥接受 PEM 或 Base64 DER；可以通过 `APP__RSA__PRIVATE_KEY` 提供，避免提交真实密钥。可选 `public_key` 保存客户端使用的对应公钥，但服务端解密只读取私钥。未启用解密提取器的接口不要求配置 RSA。

客户端提供匿名 `GET /Authorize/GetPublicKey`，通过授权应用服务原样返回 `[rsa].public_key`，默认输出纯文本。RSA 配置段不存在或为空时返回 400 与 `RSA公钥加载异常`；配置了私钥但没有公钥时返回空体 204；公钥为空字符串时保留 200 空文本。接口不校验或转换公钥格式，不返回私钥，也不回退 JWT 公钥。该接口不会自动启用登录字段解密。

默认 `AppState` 与 `Arc<AppState>` 已提供配置访问。自定义宿主状态实现 `RsaDecryptionState` 即可使用提取器。其他请求绑定方式可在绑定完成后调用 `decrypt_request(&mut model, private_key)`，不要对响应模型或注入的服务对象调用。

客户端使用对应公钥以 **OAEP-SHA256** 加密 UTF-8 字符串，再编码为 Base64。服务端不会尝试其他填充算法。空字符串也会尝试解密，`None` 跳过。

## 错误行为

- 私钥配置缺失时报告内部异常，即使模型中没有非空密文字段。
- Debug 构建遇到非法密文时只记录固定错误消息，保留该字段原值，并继续遍历。
- Release 构建遇到非法密文时停止处理，交给共享内部异常响应。Development 环境保留诊断信息，其他环境使用通用异常消息。
- 构建模式决定非法密文是否放行；运行环境只决定异常如何返回，两者互不替代。

日志不包含密钥、密文或解密后的内容。解密不替代 JWT、接口权限或请求签名。字段解密在参数绑定后、业务调用前执行；其他请求规则见[请求绑定](request-binding.md)。
