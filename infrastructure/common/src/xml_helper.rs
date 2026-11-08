use quick_xml::{
    Reader, XmlVersion, de::from_str, escape::resolve_xml_entity, events::Event, se::to_string,
};
use serde::{Serialize, de::DeserializeOwned};

/// 对象转 Xml；元素文本使用运行平台的换行符，属性保留转义后的换行。
pub fn object_to_xml<T>(value: &T) -> Result<String, quick_xml::SeError>
where
    T: Serialize,
{
    let xml = to_string(value)?;
    validate_xml_characters(&xml).map_err(|error| quick_xml::SeError::Custom(error.into()))?;
    normalize_output_newlines(&xml)
        .map_err(|_| quick_xml::SeError::Custom("XML 输出换行处理失败".into()))
}

/// Xml 转对象；与源 XmlReader 一致，禁止 DTD，不解析外部资源。
pub fn xml_to_object<T>(xml_text: &str) -> Result<T, quick_xml::DeError>
where
    T: DeserializeOwned,
{
    validate_xml_characters(xml_text).map_err(|error| quick_xml::DeError::Custom(error.into()))?;
    // 先按 XML 事件检查，不能把注释、CDATA 中的同名文本当作声明。
    // 不只依赖实体解析器：反序列化跳过未知字段时可能直接跳过整个子树。
    let mut reader = Reader::from_str(xml_text);
    loop {
        match reader.read_event()? {
            Event::DocType(_) => {
                return Err(quick_xml::DeError::Custom("XML 不允许 DTD 声明".into()));
            }
            Event::Start(element) | Event::Empty(element) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(quick_xml::Error::from)?;
                    let value = attribute.normalized_value_with(
                        XmlVersion::Implicit1_0,
                        1,
                        resolve_xml_entity,
                    )?;
                    validate_xml_characters(&value)
                        .map_err(|error| quick_xml::DeError::Custom(error.into()))?;
                }
            }
            Event::GeneralRef(reference) => match reference.resolve_char_ref()? {
                Some(character) if !is_xml_character(character) => {
                    return Err(quick_xml::DeError::Custom(INVALID_XML_CHARACTER.into()));
                }
                None if resolve_xml_entity(reference.as_ref()).is_none() => {
                    return Err(quick_xml::DeError::Custom(
                        "XML 包含未声明的实体引用".into(),
                    ));
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    from_str(xml_text)
}

const INVALID_XML_CHARACTER: &str = "XML 包含 XML 1.0 不允许的字符";

/// XML 1.0 Char 范围；不能用 Unicode 的控制字符分类替代。
fn is_xml_character(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\r'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}

fn validate_xml_characters(text: &str) -> Result<(), &'static str> {
    if text.chars().all(is_xml_character) {
        Ok(())
    } else {
        Err(INVALID_XML_CHARACTER)
    }
}

/// quick-xml 会把文本 CR 转成字符引用；源 XmlWriter 默认改为平台换行。
/// 仅处理本工具刚序列化的 XML，不能对用户输入的字符引用执行此转换。
fn normalize_output_newlines(xml: &str) -> Result<String, quick_xml::Error> {
    let mut reader = Reader::from_str(xml);
    let mut output = String::with_capacity(xml.len());
    let mut previous_cr = false;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event()?;
        let end = reader.buffer_position() as usize;
        match event {
            Event::Eof => break,
            Event::Text(text) => {
                for character in text.chars() {
                    append_output_character(&mut output, character, &mut previous_cr);
                }
            }
            Event::GeneralRef(reference) => match reference.resolve_char_ref()? {
                Some(character @ ('\r' | '\n')) => {
                    append_output_character(&mut output, character, &mut previous_cr);
                }
                _ => {
                    output.push_str(&xml[start..end]);
                    previous_cr = false;
                }
            },
            Event::CData(text) => {
                previous_cr = false;
                output.push_str("<![CDATA[");
                for character in text.chars() {
                    append_output_character(&mut output, character, &mut previous_cr);
                }
                output.push_str("]]>");
                previous_cr = false;
            }
            _ => {
                // 整个标记原样写回，属性内的 CR/LF/Tab 引用不能被文本规则改写。
                output.push_str(&xml[start..end]);
                previous_cr = false;
            }
        }
    }
    Ok(output)
}

fn append_output_character(output: &mut String, character: char, previous_cr: &mut bool) {
    if character == '\n' && *previous_cr {
        *previous_cr = false;
        return;
    }
    *previous_cr = character == '\r';
    if matches!(character, '\r' | '\n') {
        output.push_str(if cfg!(windows) { "\r\n" } else { "\n" });
    } else {
        output.push(character);
    }
}
