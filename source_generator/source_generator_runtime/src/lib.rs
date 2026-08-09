use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};
use std::hash::BuildHasher;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;

use distributed_lock::DistributedLock;
use hybrid_cache::{HybridCache, HybridCacheError};
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

/// 重新导出缓存值与显式信任键使用的序列化约束
#[doc(hidden)]
pub use serde::Serialize;

mod adapters;
mod invocation_key;
mod logging;
mod method_key;
mod options;
mod retry;

pub use adapters::*;
pub use invocation_key::*;
pub use logging::*;
pub use method_key::*;
pub use options::*;
pub use retry::*;

/// 为 async fn 方法自动注入结构化调用日志的属性宏
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
