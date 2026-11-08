use crate::guid;
use indexmap::IndexMap;
mod cell;
mod enumeration;
mod input;
mod table;
pub use cell::DataCell;
pub use enumeration::{DataEnum, EnumMember, EnumProperty, EnumRepr, EnumSnapshot, EnumValue};
pub use guid::parse as parse_guid;
pub use input::{DataModel, FromDataCell};
pub use source_generator_core::{DataEnum, DataModel};
pub(crate) use table::unwrapped_type_name;
pub use table::{DataColumn, DataColumnType, DataTable};

/// 数据表行
pub type DataTableRow = IndexMap<String, DataCell>;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(feature = "runtime")]
pub use runtime::*;
