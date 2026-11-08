use crate::data_helper::DataCell;
use indexmap::IndexMap;
use serde::{de::DeserializeOwned, ser::Error};

mod model;
pub use model::{Property, PropertyKind, PropertyModel, PropertyResult};
#[doc(hidden)]
pub use serde::Serialize;
pub use source_generator_core::PropertyModel;

/// 按属性名读取模板路径，错误保留调用方传入的完整路径。
pub fn get_property<T: PropertyModel + ?Sized>(value: &T, field: &str) -> PropertyResult<Property> {
    let path = field.split('.').collect::<Vec<_>>();
    value
        .read_property(&path)?
        .ok_or_else(|| serde_json::Error::custom(format!("{field}字段不存在")))
}

/// 获取对象的属性字典
pub fn get_properties<T>(value: &T) -> PropertyResult<IndexMap<String, DataCell>>
where
    T: PropertyModel,
{
    properties_to_map(value, false)
}

/// 获取对象的属性显示名字典
pub fn get_properties_display_name<T>(value: &T) -> PropertyResult<IndexMap<String, DataCell>>
where
    T: PropertyModel,
{
    properties_to_map(value, true)
}

fn properties_to_map<T: PropertyModel>(
    value: &T,
    display_names: bool,
) -> PropertyResult<IndexMap<String, DataCell>> {
    let mut result = IndexMap::new();
    for property in value.properties()? {
        let Some(value) = property.scalar_cell()? else {
            continue;
        };
        let name = if display_names {
            property.display_name.unwrap_or(property.name)
        } else {
            property.name
        };
        if result.insert(name.to_owned(), value).is_some() {
            return Err(serde_json::Error::custom(format!("属性名称重复：{name}")));
        }
    }
    Ok(result)
}

/// 比较两个实体并输出差异文本
pub fn comparison_entity<T>(original: &T, after: &T) -> PropertyResult<String>
where
    T: PropertyModel,
{
    let original = original.properties()?;
    let after = after.properties()?;
    let mut lines = Vec::new();
    for property in original {
        let other = after
            .iter()
            .find(|other| other.name == property.name)
            .ok_or_else(|| serde_json::Error::custom(format!("属性不存在：{}", property.name)))?;
        if property.value_equals(other)? {
            continue;
        }
        let left = property.display_value()?;
        let right = other.display_value()?;
        let arrow = if property.kind == PropertyKind::DateTime {
            " ->"
        } else {
            " -> "
        };
        lines.push(format!("{}:{left}{arrow}{right}； \n", property.name));
    }

    Ok(lines.concat())
}

/// 同类型赋值
pub fn assignment<T>(left: &mut T, right: &T) -> Result<(), serde_json::Error>
where
    T: Serialize + DeserializeOwned,
{
    *left = clone(right)?;
    Ok(())
}

/// 克隆对象
pub fn clone<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: Serialize + DeserializeOwned,
{
    let payload = serde_json::to_vec(value)?;
    serde_json::from_slice(&payload)
}

/// 不同类型赋值
pub fn assignment_different_type<L, R>(left: &mut L, right: &R) -> Result<(), serde_json::Error>
where
    L: DeserializeOwned,
    R: Serialize,
{
    let payload = serde_json::to_vec(right)?;
    *left = serde_json::from_slice(&payload)?;
    Ok(())
}
