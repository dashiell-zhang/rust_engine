use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};
use std::hash::BuildHasher;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;

use distributed_cache::{CacheEntryOptions, CacheError, DistributedCache};
use distributed_lock::DistributedLock;
use serde::{
    de::DeserializeOwned,
    ser::{
        Error as SerializeError, SerializeMap, SerializeSeq, SerializeStruct,
        SerializeStructVariant, SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
        Serializer,
    },
};
use sha2::{Digest, Sha256};
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Write};
use std::sync::Arc;

/// 重新导出分布式锁错误，供属性宏在调用方保留底层错误类型
pub use distributed_lock::LockError;

/// 取消错误与令牌契约，供重试宏及自定义错误实现使用。
pub use cancellation::{CancellationError, CancellationToken, OperationCancelled};

/// 重新导出缓存值与显式信任键使用的序列化约束
#[doc(hidden)]
pub use serde::Serialize;

mod adapters;
mod cacheable;
mod invocation_key;
mod lease;
mod logging;
mod logging_snapshot;
mod logging_stream;
mod method_key;
mod options;
mod pipeline;
mod retry;

pub use adapters::*;
pub use cacheable::*;
pub use invocation_key::*;
pub use lease::*;
pub use logging::*;
pub use logging_snapshot::*;
pub use logging_stream::*;
pub use method_key::*;
pub use options::*;
pub use pipeline::*;
pub use retry::*;

/// 显式自定义调用行为，支持同步、异步以及延迟枚举的流。
pub use source_generator_core::behavior;
/// 组合显式行为并共享一次调用的元数据；不会隐式挂载日志或重试。
pub use source_generator_core::invocation;
pub use source_generator_core::{endpoint, webapi_endpoint};

/// 为同步或异步方法自动注入结构化调用日志的属性宏
///
/// 详见 source_generator_core::logging
pub use source_generator_core::logging;

/// 为结构体或枚举派生稳定调用键实现
///
/// 详见 source_generator_core::StableInvocationKey
pub use source_generator_core::StableInvocationKey;

/// 为 async fn 方法开启自动重试的属性宏
///
/// 适合参数为 `Copy` 类型或引用的方法
/// 若方法体在 `.await` 点跨越移动了 owned 参数，编译器会报错，此时应改为传引用或在调用侧处理重试
/// 错误默认通过 CancellationError 识别取消；cancellation 参数可中断重试间隔，
/// 此时错误类型还须实现 From<OperationCancelled>。AppError 和 LlmError 已提供这些实现。
///
/// 详见 source_generator_core::retry
pub use source_generator_core::retry;

/// 为 async fn 方法开启缓存包装的属性宏
///
/// 详见 source_generator_core::cacheable
pub use source_generator_core::cacheable;

/// 为 async fn 方法开启并发限制包装的属性宏
///
/// 详见 source_generator_core::concurrency_limit
pub use source_generator_core::concurrency_limit;

/// 生成应用服务工厂方法的函数宏
///
/// 详见 source_generator_core::generate_service_factory
pub use source_generator_core::generate_service_factory;

/// 生成直接服务工厂方法的函数宏
///
/// 详见 source_generator_core::generate_direct_service_factory
pub use source_generator_core::generate_direct_service_factory;

/// 生成任务注册表批量注册语句的函数宏
///
/// 详见 source_generator_core::register_task_registry
pub use source_generator_core::register_task_registry;

#[doc(hidden)]
pub use event_bus;
/// 事件协议声明宏及宏展开所需的契约路径。
pub use source_generator_core::Event;

/// 请求模型的 RSA 字段遍历契约。
pub mod rsa;
pub use rsa::RsaDecrypt;
pub use source_generator_core::RsaDecrypt;

pub use source_generator_core::DataEnum;
pub use source_generator_core::DataModel;
/// 公共属性与数据导出所用的字段元数据；调用方需依赖 common。
pub use source_generator_core::PropertyModel;
pub use source_generator_core::ProtocolModel;
pub use source_generator_core::model_documentation;
