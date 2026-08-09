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

use attributes::{cacheable::*, concurrency_limit::*, logging::*, retry::*, scoped_key::*};
use derive::stable_invocation_key::*;
use generators::{service_factory::*, task_registry::*};
use method_key::*;
use runtime_path::*;
use syntax::*;

// ─── logging ─────────────────────────────────────────────────────────────────

/// 为 async fn 方法自动注入结构化调用日志
///
/// 在方法执行前记录 executing 日志，执行后记录 executed 日志（含耗时），
/// 发生异常时记录 exception 日志（含耗时与错误描述）
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug
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

/// 为 async fn 方法开启自动重试，返回 Err 时按配置次数重试
///
/// - `max_retries`：最大重试次数，默认 3
/// - `delay_seconds`：每次重试前等待秒数，默认 0
/// - `delay_ms`：Rust 侧扩展的毫秒级等待配置，不能与 `delay_seconds` 同时使用
///
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 Debug
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
/// - cache：缓存实例表达式，支持 Arc<HybridCache> 或 &HybridCache
/// - ttl_seconds：L1 与 L2 共同遵守的缓存有效期秒数，默认 60
/// - lock：可选的分布式锁实例表达式，用于缓存回源防击穿
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
/// 仅支持返回 Result<T, E> 的 async fn，T 需实现 Serialize、DeserializeOwned 与 PartialEq
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
/// - semaphore：允许的最大并发数，默认 1，仅接受整数值字面量
/// - expiry_seconds：锁过期秒数，默认 0 表示使用锁实现默认值，仅接受整数值字面量
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
/// 仅支持返回 Result<T, E> 的 async fn，E 需实现 From<String> 与 From<LockError>
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
#[proc_macro]
pub fn generate_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ServiceFactoryInput);

    expand_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为具体适配器生成直接服务工厂方法
///
/// 适用于非泛型状态提供器场景，例如 TaskServiceApplicationAdapter
#[proc_macro]
pub fn generate_direct_service_factory(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DirectServiceFactoryInput);

    expand_direct_service_factory(input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// 为任务注册表生成批量注册语句
///
/// 通过声明式清单先注册 queue schedule 再登记 alias，并返回注册结果
#[proc_macro]
pub fn register_task_registry(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as TaskRegistryRegistrationInput);

    expand_task_registry_registration(input).unwrap_or_else(|e| e.to_compile_error().into())
}
