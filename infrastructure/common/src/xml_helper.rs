use quick_xml::{de::from_str, se::to_string};
use serde::{Serialize, de::DeserializeOwned};

/// 对象转 Xml
pub fn object_to_xml<T>(value: &T) -> Result<String, quick_xml::SeError>
where
    T: Serialize,
{
    to_string(value)
}

/// Xml 转对象
pub fn xml_to_object<T>(xml_text: &str) -> Result<T, quick_xml::DeError>
where
    T: DeserializeOwned,
{
    from_str(xml_text)
}
