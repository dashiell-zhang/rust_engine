use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, Ident, ItemFn, MetaNameValue, Pat, Path,
    PathArguments, Token, Type, braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    visit::{self, Visit},
};

mod attributes;
mod derive;
mod generators;
mod method_key;
mod runtime_path;
mod syntax;

/// 为同一方法的显式横切属性建立共享上下文，按声明顺序进入、逆序退出。
#[proc_macro_attribute]
pub fn invocation(attr: TokenStream, item: TokenStream) -> TokenStream {
    attributes::invocation::expand(
        parse_macro_input!(attr as attributes::invocation::InvocationArgs),
        parse_macro_input!(item as ItemFn),
    )
    .unwrap_or_else(|error| error.to_compile_error())
    .into()
}

/// 为 SeaORM Model 声明软删除字段及默认活跃查询入口。
#[proc_macro_derive(SoftDelete)]
pub fn derive_soft_delete(input: TokenStream) -> TokenStream {
    derive::soft_delete::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

/// 自定义调用管道：behavior = 表达式 或 behaviors(外层, 内层)。
/// 同步行为接收 FnOnce，异步行为接收 Future，stream = true 接收 Stream。
/// 流的准备失败直接返回；Result 包装的流显式使用 result = true。
/// 沿用 key_scope、key_types、key_consts 和 key_include_self 的稳定键规则。
#[proc_macro_attribute]
pub fn behavior(attr: TokenStream, item: TokenStream) -> TokenStream {
    attributes::behavior::expand(
        parse_macro_input!(attr as attributes::behavior::BehaviorArgs),
        parse_macro_input!(item as ItemFn),
    )
    .unwrap_or_else(|error| error.to_compile_error())
    .into()
}

/// 协议对象忽略字段 ASCII 大小写，按出现顺序验证每次赋值，合法重复字段取末值。
/// 支持具名字段、serde(rename)、serde(default)；不应用 HTTP 请求模型的转换规则。
#[proc_macro_derive(ProtocolModel, attributes(serde, protocol))]
pub fn derive_protocol_model(input: TokenStream) -> TokenStream {
    derive::protocol_model::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 把实体及字段的文档转换为 SeaORM 表、列注释；放在 DeriveEntityModel 之前。
/// 显式 comment 优先，缺少文档时使用映射名称；忽略 xmin 和非持久化字段。
#[proc_macro_attribute]
pub fn model_documentation(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "模型文档属性不接受参数")
            .to_compile_error()
            .into();
    }
    attributes::model_documentation::expand(parse_macro_input!(item as syn::ItemStruct))
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

/// 生成 Query 字段绑定与错误收集，沿用 Serde 声明；支持 query(validate = "函数路径")。
#[proc_macro_derive(QueryModel, attributes(query, serde))]
pub fn derive_query_model(input: TokenStream) -> TokenStream {
    derive::query_model::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

use attributes::{cacheable::*, concurrency_limit::*, logging::*, retry::*, scoped_key::*};
use derive::stable_invocation_key::*;
use generators::{service_factory::*, task_registry::*};
use method_key::*;
use runtime_path::*;
use syntax::*;

// ─── logging ─────────────────────────────────────────────────────────────────

/// 为同步或异步方法自动注入结构化调用日志
///
/// 在方法执行前记录 executing 日志，执行后记录 executed 日志（含耗时），
/// 发生异常时记录 exception 日志（含耗时与错误描述）
///
/// 默认按 Result<T, E> 处理返回值，E 需实现 Debug；普通返回值或单元值使用 result = false
/// 参数与结果优先生成独立 Serde 快照，无法序列化时回退 Display 或类型名称
/// 泛型值要记录字段时，应在声明处提供 Serialize 约束
/// 调用后刷新引用参数；返回值仍借用可变参数时可使用 refresh_arguments = false
/// stream = true 时在实际消费期间记录流生命周期，最多捕获前 100 项
/// 流默认逐项处理 Result，普通元素使用 stream_item_result = false
/// 常见 BoxStream/LocalBoxStream/Pin<Box<_>> 返回类型自动装箱，自定义别名使用 stream_boxed
/// 必须通过 key_scope 声明跨构建稳定且全局唯一的方法标识
/// receiver 方法必须显式配置 key_include_self，普通参数类型不参与方法键
/// 方法自身泛型与参数位置的 impl Trait 会加入实际类型维度并需声明 StableKeyType 约束
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::logging;
///
/// impl MyService {
///     #[logging(
///         key_scope = "application.my-service.get-user",
///         key_include_self = false
///     )]
///     pub async fn get_user(&self, id: i64) -> Result<UserDto, AppError> {
///         // 方法体
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn logging(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as LoggingArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_logging(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

// ─── retry ───────────────────────────────────────────────────────────────────

/// 为 async fn 方法开启自动重试，非取消错误按配置次数重试
///
/// - `max_retries`：最大重试次数，默认 3
/// - `delay_seconds`：每次重试前等待秒数，默认 0
/// - `delay_ms`：Rust 侧扩展的毫秒级等待配置，不能与 `delay_seconds` 同时使用
/// - `cancellation`：可选的 CancellationToken 表达式，用于中断重试等待
/// - `is_cancelled`：可选的 Fn(&E) -> bool 表达式，用于自定义错误的取消分类
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug 和 CancellationError；
/// 提供 is_cancelled 时不要求 CancellationError。提供 cancellation 时还需
/// E: From<OperationCancelled>，等待被取消时使用该转换，不以文本构造普通错误。
/// 取消错误直接传播；次数耗尽优先返回最后一次业务错误；不主动取消执行中的方法体。
///
/// # 使用限制
///
/// receiver 只支持不可变引用，其他参数必须实现 `Copy`，且不支持可变绑定、可变引用或可变裸指针。
/// 必须通过 key_scope 声明跨构建稳定且全局唯一的方法标识。
/// receiver 方法必须显式配置 key_include_self，普通参数类型不参与方法键。
/// 方法自身泛型与参数位置的 impl Trait 会加入实际类型维度并需声明 StableKeyType 约束。
/// 需要在单次尝试中修改参数时，应在方法体内从不可变原参数创建局部副本或克隆，
/// 使该局部值能够在每次重试执行方法体时重新初始化。
///
/// # 示例
/// ```ignore
/// use source_generator_runtime::retry;
///
/// impl MyService {
///     // 参数均为 Copy 类型或引用，适合使用 retry
///     #[retry(
///         max_retries = 3,
///         delay_seconds = 1,
///         key_scope = "application.my-service.call-external",
///         key_include_self = false
///     )]
///     pub async fn call_external(&self, id: i64) -> Result<String, AppError> {
///         // 外部调用可能失败
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn retry(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RetryArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_retry(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为 async fn 方法开启缓存包装，命中缓存时直接返回序列化结果
///
/// - cache：分布式缓存实例表达式，支持实现、引用或 Arc
/// - ttl_seconds：分布式缓存有效期秒数，默认 60
/// - lock：缓存返回值时必填的分布式锁实例表达式，用于缓存回源防击穿
/// - lock_expiry_seconds：防击穿锁有效期秒数，默认 60
/// - key_scope：必填的稳定方法键
/// - key_types(...)：加入实现 StableKeyType 的外围类型键维度，方法自身类型泛型会自动加入
/// - key_consts(...)：显式加入外围 const 泛型等宏不可见的常量键维度，方法自身 const 泛型会自动加入
/// - key_include_self：receiver 方法必须显式声明是否通过 StableKeyType 将 Self 加入键
/// - key：可选的显式参数键表达式，包含所有业务相关参数且不得移动方法参数
/// - cache_value：返回类型别名等无法静态分析时，显式声明是否缓存成功值
///
/// 显式参数键应优先写成 `key = (&id, &tenant_id)`，无序集合应使用
/// `key = source_generator_runtime::unordered_key(&ids)` 明确声明集合语义
/// 未配置 key 时，除 receiver 外的全部参数都必须实现 StableInvocationKey
/// 配置 key 后只要求该表达式实现 StableInvocationKey，调用方负责包含所有影响结果的稳定业务维度
/// StableInvocationKey 不支持可变引用，派生类型中的嵌套可变引用也会被拒绝
/// 稳定参数键生成失败时会记录降级日志并绕过缓存
/// Result<(), E> 或显式 cache_value = false 时不会生成缓存代码，也不会引入缓存相关 Trait 约束
///
/// 仅支持返回 Result<T, E> 的 async fn，T 需实现 Serialize 与 DeserializeOwned
/// receiver 必须是不可变引用，其他参数只支持不带子模式的标识符绑定
/// 启用缓存值时不支持包含可变引用的参数
#[proc_macro_attribute]
pub fn cacheable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as CacheableArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_cacheable(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为 async fn 方法开启并发限制包装，使用分布式锁控制同 key 并发数
///
/// - lock：必填的分布式锁实例表达式
/// - is_use_parameter：是否将参数加入锁 key，仅接受布尔字面量
/// - is_block：获取不到锁时是否立即返回错误，仅接受布尔字面量
/// - semaphore：允许的最大并发数，非正值按 1 处理，仅接受整数值字面量
/// - expiry_seconds：锁过期秒数，非正值使用 60 秒租约，仅接受整数值字面量
/// - key_scope：必填的稳定方法键
/// - key_types(...)：加入实现 StableKeyType 的外围类型键维度，方法自身类型泛型会自动加入
/// - key_consts(...)：显式加入外围 const 泛型等宏不可见的常量键维度，方法自身 const 泛型会自动加入
/// - key_include_self：receiver 方法必须显式声明是否通过 StableKeyType 将 Self 加入键
/// - key：is_use_parameter 为 true 时可使用的显式参数键表达式
///
/// 显式参数键应包含全部会影响并发隔离的业务维度，并通过引用避免移动方法参数
/// 无序集合应使用 source_generator_runtime::unordered_key 显式声明集合语义
/// 未配置 key 时，除 receiver 外的全部参数都必须实现 StableInvocationKey
/// 可变引用参数应在显式 key 中通过 &*value 创建不可变重借用
/// 参数键生成失败时会返回错误，禁止绕过锁继续执行
///
/// 持锁期间自动续期，正常结束等待释放；阻断和参数键错误通过 LockError 保留内部错误分类
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 From<LockError>
#[proc_macro_attribute]
pub fn concurrency_limit(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ConcurrencyLimitArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_concurrency_limit(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为结构体或枚举生成完整字段稳定调用键实现
///
/// 必须通过 `scope = "..."` 声明稳定类型作用域。默认按照非递归字段的完整声明类型
/// 生成 `StableInvocationKey` 约束；直接递归字段应使用当前类型的非限定路径或
/// `self::Type` 路径，完全限定路径、类型别名、复杂关联类型或自定义递归包装器
/// 可以通过 `bound = "..."` 替换自动字段约束。所有类型泛型仍需实现
/// `StableKeyType`，以便写入派生类型自身的稳定泛型维度。
#[proc_macro_derive(StableInvocationKey, attributes(invocation_key))]
pub fn derive_stable_invocation_key(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    expand_stable_invocation_key(input).unwrap_or_else(|error| error.to_compile_error().into())
}

/// 生成应用服务工厂方法
///
/// 通过声明式清单生成按上下文分组的工厂方法，减少重复的 `Service::new(context)` 样板代码
/// 注册项可追加 `=> dyn Contract + 'a`，返回仅公开契约的 `Box<dyn Contract + 'a>`。
#[proc_macro]
pub fn generate_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ServiceFactoryInput);

    expand_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为具体适配器生成直接服务工厂方法
///
/// 适用于非泛型状态提供器场景，例如 WebApiApplicationAdapter
/// 注册项可追加 `=> dyn Contract + 'a`，由适配器选择实现，调用方只获得契约。
#[proc_macro]
pub fn generate_direct_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DirectServiceFactoryInput);

    expand_direct_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为任务注册表生成批量注册语句
///
/// 通过 definition/factory 清单先注册 queue schedule，再登记 alias；工厂每次执行时调用
#[proc_macro]
pub fn register_task_registry(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as TaskRegistryRegistrationInput);

    expand_task_registry_registration(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为非泛型结构体声明稳定事件协议名，要求显式 event(name = "...")。
#[proc_macro_derive(Event, attributes(event))]
pub fn derive_event(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    derive::event::expand_event(input).unwrap_or_else(|error| error.to_compile_error().into())
}

/// 从 SeaORM 模型生成分区声明，要求预分配的单列 i64 主键。
#[proc_macro_derive(PartitionTable, attributes(partition, sea_orm))]
pub fn derive_partition_table(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    derive::partition::expand_partition(input)
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 为请求模型生成 RSA 标记字段及嵌套对象的遍历，不处理未标记的字符串。
#[proc_macro_derive(RsaDecrypt, attributes(rsa_encrypted, rsa))]
pub fn derive_rsa_decrypt(input: TokenStream) -> TokenStream {
    derive::rsa::expand_rsa_decrypt(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 按声明顺序读取公共字段，支持 property(name/display_name/value_type/skip/read_only/nested/enumeration)。
/// 展开契约位于 common::property_helper，与 Serde 的字段重命名独立。
/// 模型可通过 property(model_name = "...") 指定数据表名称。
#[proc_macro_derive(PropertyModel, attributes(property))]
pub fn derive_property_model(input: TokenStream) -> TokenStream {
    derive::property_model::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 为正文模型生成显式验证元数据，保留 Serde 字段命名和默认值。
#[proc_macro_derive(JsonModel, attributes(json, serde))]
pub fn derive_json_model(input: TokenStream) -> TokenStream {
    derive::json_model::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 为数据表导入生成字段与显示名称匹配，read_only 字段不赋值。
#[proc_macro_derive(DataModel, attributes(property))]
pub fn derive_data_model(input: TokenStream) -> TokenStream {
    derive::property_model::expand_data_model(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}

/// 数值枚举元数据，支持类型 flags 与成员 name/description，保留显式整数 repr。
#[proc_macro_derive(DataEnum, attributes(data_enum))]
pub fn derive_data_enum(input: TokenStream) -> TokenStream {
    derive::data_enum::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|error| error.to_compile_error().into())
}
