//! 读取原始 JSON token 时保留字段路径和形态错误；不从 Serde 错误字符串反推类型。
use super::json_key::{Key, KeySeed};
use super::{
    Input, InputError, JsonField, JsonFieldKind, JsonModel, JsonObject, JsonScalar,
    JsonScalarField, Source, Value, json_model,
};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub enum JsonReadError {
    Depth {
        maximum: usize,
        container: &'static str,
        path: String,
        line: usize,
        column: usize,
    },
    Shape {
        type_name: String,
        path: String,
        line: usize,
        column: usize,
    },
    Syntax {
        error: serde_json::Error,
        path: String,
    },
    Trailing {
        byte: u8,
        line: usize,
        column: usize,
    },
    EndAfterSeparator {
        path: String,
        line: usize,
        column: usize,
    },
    UnterminatedString {
        path: String,
        line: usize,
        column: usize,
    },
    Conversion(InputError),
    Validation(Vec<InputError>),
}

pub fn read_json_model<T: JsonModel>(bytes: &[u8]) -> Result<Option<T>, JsonReadError> {
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let mut state = State::default();
    let parsed = Seed {
        shape: Shape::Object(JsonObject {
            name: T::json_type_name(),
            fields: T::json_fields(),
        }),
        state: &mut state,
    }
    .deserialize(&mut reader);
    let mut value = parsed.map_err(|error| {
        let path = state.path();
        if let Some(container) = state.depth_error {
            let (line, column) =
                depth_position(bytes).unwrap_or((error.line().saturating_sub(1), error.column()));
            return JsonReadError::Depth {
                maximum: super::super::MAX_DEPTH,
                container,
                path,
                line,
                column,
            };
        }
        match state.type_error {
            Some(type_name) => {
                let (line, column) = if state.scoped {
                    scoped_position(bytes, &error, state.opening)
                } else {
                    shape_position(bytes, &error, state.opening)
                };
                JsonReadError::Shape {
                    type_name,
                    path,
                    line,
                    column,
                }
            }
            None => {
                if error.is_eof()
                    && state.awaiting_entry
                    && bytes
                        .iter()
                        .rfind(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                        == Some(&b',')
                {
                    // 源读取器在逗号恰为末字节时回退一字节；有后续空白则停在 EOF。
                    let offset = bytes.len() - usize::from(bytes.last() == Some(&b','));
                    let (line, column) = byte_position(bytes, offset);
                    return JsonReadError::EndAfterSeparator { path, line, column };
                }
                if error.is_eof() && ends_in_unterminated_string(bytes) {
                    let (line, column) = byte_position(bytes, bytes.len());
                    return JsonReadError::UnterminatedString { path, line, column };
                }
                JsonReadError::Syntax { error, path }
            }
        }
    })?;
    if let Err(error) = reader.end() {
        // end 只消费 JSON 空白；通过现有读取器的位置取失败字节，不再解析一次正文
        let offset = reader.into_iter::<de::IgnoredAny>().byte_offset();
        let Some(&byte) = bytes.get(offset) else {
            return Err(JsonReadError::Syntax {
                error,
                path: "$".into(),
            });
        };
        let (line, column) = byte_position(bytes, offset);
        return Err(JsonReadError::Trailing { byte, line, column });
    }
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let mut errors = Vec::new();
    json_model::prepare(&mut value, T::json_fields(), "", &mut errors);
    let value = T::deserialize(Input {
        value,
        source: Source::Json,
    })
    .map_err(JsonReadError::Conversion)?;
    if errors.is_empty() {
        Ok(Some(value))
    } else {
        Err(JsonReadError::Validation(errors))
    }
}

fn byte_position(bytes: &[u8], offset: usize) -> (usize, usize) {
    let prefix = &bytes[..offset];
    (
        prefix.iter().filter(|byte| **byte == b'\n').count(),
        prefix
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(offset, |index| offset - index - 1),
    )
}

/// 仅在原读取器已确认 EOF 后辨认字符串边界，不读取模型、不替代 JSON 解析。
/// 非法转义或控制字符不能归类为缺少引号；尤其短 Unicode 转义可能先被 Serde 报 EOF。
fn ends_in_unterminated_string(bytes: &[u8]) -> bool {
    enum Token {
        Outside,
        Text,
        Escape,
        Hex(u8),
    }
    let mut token = Token::Outside;
    for &byte in bytes {
        token = match token {
            Token::Outside => {
                if byte == b'"' {
                    Token::Text
                } else {
                    Token::Outside
                }
            }
            Token::Text => match byte {
                b'"' => Token::Outside,
                b'\\' => Token::Escape,
                0..=0x1f => return false,
                _ => Token::Text,
            },
            Token::Escape => match byte {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => Token::Text,
                b'u' => Token::Hex(4),
                _ => return false,
            },
            Token::Hex(remaining) => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
                if remaining == 1 {
                    Token::Text
                } else {
                    Token::Hex(remaining - 1)
                }
            }
        };
    }
    !matches!(token, Token::Outside)
}

#[derive(Default)]
struct State {
    segments: Vec<String>,
    type_error: Option<String>,
    opening: Option<u8>,
    depth: usize,
    depth_error: Option<&'static str>,
    scoped: bool,
    awaiting_entry: bool,
}

/// 只有 MapAccess 尚未进入键读取时才保留等待标记，未闭合的键字符串不能误判。
struct TrackedKeySeed<'a> {
    read_property: bool,
    state: &'a mut State,
}

impl<'de> DeserializeSeed<'de> for TrackedKeySeed<'_> {
    type Value = Key;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Key, D::Error> {
        self.state.awaiting_entry = false;
        KeySeed {
            read_property: self.read_property,
        }
        .deserialize(deserializer)
    }
}

/// 仅在 token 读取已确认超深后定位起始括号；不提前扫描并覆盖更早的绑定错误。
/// MapAccess 可能已消费第一个键，不能用其当前位置当作对象起点。
fn depth_position(bytes: &[u8]) -> Option<(usize, usize)> {
    let (mut line, mut column, mut depth) = (0, 0, 0usize);
    let (mut in_string, mut escaped) = (false, false);
    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'{' | b'[' => {
                    if depth >= super::super::MAX_DEPTH {
                        return Some((line, column));
                    }
                    depth += 1;
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if byte == b'\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    None
}

fn shape_position(bytes: &[u8], error: &serde_json::Error, opening: Option<u8>) -> (usize, usize) {
    let line = error.line().saturating_sub(1);
    let Some(opening) = opening else {
        return (line, error.column());
    };
    let start = bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .nth(line.saturating_sub(1))
        .filter(|_| line > 0)
        .map_or(0, |(index, _)| index + 1);
    let end = (start + error.column()).min(bytes.len());
    // Serde 在 visit_map/seq 返回错误后仍会跳过空白和消费结束符；这些字节不属于失败 token。
    // 拒绝形态时没有读取子值，因此该范围内最后一个同类开始符就是失败 token。
    let end = bytes[..end]
        .iter()
        .rposition(|byte| *byte == opening)
        .map_or(end, |index| index + 1);
    let line = bytes[..end].iter().filter(|byte| **byte == b'\n').count();
    let column = bytes[..end]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(end, |index| end - index - 1);
    (line, column)
}

/// 可空转换器重新建立当前值的读取器；合法标量不含实际换行，错误列是原始 token 长度
fn scoped_position(bytes: &[u8], error: &serde_json::Error, opening: Option<u8>) -> (usize, usize) {
    if opening.is_some() {
        return (0, 1);
    }
    let line = error.line().saturating_sub(1);
    let start = bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .nth(line.saturating_sub(1))
        .filter(|_| line > 0)
        .map_or(0, |(index, _)| index + 1);
    let end = (start + error.column()).min(bytes.len());
    let mut index = end.saturating_sub(1);
    if bytes.get(index) == Some(&b'"') {
        while index > 0 {
            index -= 1;
            if bytes[index] != b'"' {
                continue;
            }
            let escapes = bytes[..index]
                .iter()
                .rev()
                .take_while(|byte| **byte == b'\\')
                .count();
            if escapes % 2 == 0 {
                return (0, end - index);
            }
        }
    } else {
        while index > 0
            && !matches!(
                bytes[index - 1],
                b' ' | b'\t' | b'\r' | b'\n' | b':' | b',' | b'[' | b'{'
            )
        {
            index -= 1;
        }
    }
    (0, end - index)
}

impl State {
    fn enter<E: de::Error>(&mut self, container: &'static str) -> Result<(), E> {
        if self.depth >= super::super::MAX_DEPTH {
            self.depth_error = Some(container);
            return Err(E::custom("JSON 嵌套深度不能超过 64"));
        }
        self.depth += 1;
        Ok(())
    }
    fn path(&self) -> String {
        if self.scoped {
            "$".to_owned()
        } else {
            format!("${}", self.segments.concat())
        }
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Any,
    Scalar(JsonScalarField),
    String,
    Boolean,
    Object(JsonObject),
    Collection(&'static JsonField),
    Dictionary(&'static JsonField),
}

impl Shape {
    fn scoped(self) -> bool {
        matches!(self, Self::Scalar(JsonScalarField { scoped: true, .. }))
    }
    fn field(field: &'static JsonField) -> Self {
        if let Some(scalar) = field.scalar {
            return Self::Scalar(scalar);
        }
        match field.kind {
            JsonFieldKind::String => Self::String,
            JsonFieldKind::Boolean => Self::Boolean,
            JsonFieldKind::Object => field
                .nested
                .map_or(Self::Any, |nested| Self::Object(nested())),
            JsonFieldKind::Collection => Self::Collection(field),
            JsonFieldKind::Dictionary => Self::Dictionary(field),
            JsonFieldKind::Value => Self::Any,
        }
    }

    fn name(self) -> String {
        match self {
            Self::Any => unreachable!("动态值不产生形态错误"),
            Self::Scalar(scalar) => scalar.type_name.to_owned(),
            Self::String => "System.String".to_owned(),
            Self::Boolean => "System.Boolean".to_owned(),
            Self::Object(object) => object.name.to_owned(),
            Self::Collection(field) | Self::Dictionary(field) => {
                field.type_name.expect("集合或字典声明协议类型")()
            }
        }
    }

    fn element(field: &'static JsonField) -> Self {
        if let Some(scalar) = field.element_scalar {
            return Self::Scalar(scalar);
        }
        field.nested.map_or_else(
            || match field.element_kind {
                JsonFieldKind::String => Self::String,
                JsonFieldKind::Boolean => Self::Boolean,
                _ => Self::Any,
            },
            |nested| Self::Object(nested()),
        )
    }
}

fn property_segment(name: &str) -> String {
    // 与源 ReadStack 相同，仅这些字符触发括号形式，不改写或转义属性原文。
    const SPECIAL: &str = ". '/\"[]()\t\n\r\u{000c}\u{0008}\\\u{0085}\u{2028}\u{2029}";
    if name.chars().any(|character| SPECIAL.contains(character)) {
        format!("['{name}']")
    } else {
        format!(".{name}")
    }
}

struct Seed<'a> {
    shape: Shape,
    state: &'a mut State,
}

impl Seed<'_> {
    fn reject<E: de::Error>(&mut self) -> E {
        self.state.scoped |= self.shape.scoped();
        self.state.type_error = Some(self.shape.name());
        E::custom("JSON token 与模型声明的类型不匹配")
    }

    fn scalar<E: de::Error>(mut self, value: Value) -> Result<Value, E> {
        if let Shape::Scalar(scalar) = self.shape {
            return if (scalar.read)(JsonScalar(value.clone())).is_ok() {
                Ok(value)
            } else {
                Err(self.reject())
            };
        }
        if matches!(self.shape, Shape::Any)
            || matches!((&self.shape, &value), (Shape::String, Value::String(_)))
            || matches!((&self.shape, &value), (Shape::Boolean, Value::Bool(_)))
        {
            Ok(value)
        } else {
            Err(self.reject())
        }
    }
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        self.state.awaiting_entry = false;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON 值")
    }
    fn visit_unit<E: de::Error>(mut self) -> Result<Value, E> {
        if matches!(self.shape, Shape::Scalar(_)) {
            return self.scalar(Value::Null);
        }
        if matches!(self.shape, Shape::Boolean) {
            Err(self.reject())
        } else {
            Ok(Value::Null)
        }
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        self.scalar(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        self.scalar(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        self.scalar(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("不支持非有限 JSON 数字"))?;
        self.scalar(Value::Number(number))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.visit_string(value.to_owned())
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        self.scalar(Value::String(value))
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Value, A::Error> {
        let accepts_object = matches!(
            self.shape,
            Shape::Any | Shape::Object(_) | Shape::Dictionary(_)
        ) || self.shape.scoped();
        self.state.awaiting_entry = true;
        let first = map.next_key_seed(TrackedKeySeed {
            read_property: accepts_object,
            state: self.state,
        });
        if matches!(first, Ok(Some(Key::Number))) {
            let text = map.next_value::<String>()?;
            let number = text.parse().map_err(de::Error::custom)?;
            return self.scalar(Value::Number(number));
        }
        self.state.scoped |= self.shape.scoped();
        // 任意精度数字也通过 MapAccess 传递，只有真实对象才增加深度。
        self.state.enter("object")?;
        let fields = match self.shape {
            Shape::Any | Shape::Dictionary(_) => &[][..],
            shape if shape.scoped() => &[][..],
            Shape::Object(object) => object.fields,
            _ => {
                self.state.opening = Some(b'{');
                return Err(self.reject());
            }
        };
        let mut entries = Vec::new();
        let mut pending = first?;
        // 动态文档与未知属性由一个转换器整体读取/跳过，不为其内部节点增加读取栈路径。
        let track_children = !matches!(self.shape, Shape::Any) && !self.shape.scoped();
        while let Some(key) = pending {
            let Key::Property(key) = key else {
                return Err(de::Error::custom("JSON 属性协议无效"));
            };
            let shape = match self.shape {
                Shape::Dictionary(field) => Shape::element(field),
                _ => fields
                    .iter()
                    .find(|field| {
                        field
                            .names
                            .iter()
                            .any(|name| crate::string_helper::eq_ordinal_ignore_case(name, &key))
                    })
                    .map_or(Shape::Any, Shape::field),
            };
            if track_children {
                self.state.segments.push(property_segment(&key));
            }
            let value = map.next_value_seed(Seed {
                shape,
                state: self.state,
            })?;
            if track_children {
                self.state.segments.pop();
            }
            entries.push((key, value));
            self.state.awaiting_entry = true;
            pending = map.next_key_seed(TrackedKeySeed {
                read_property: true,
                state: self.state,
            })?;
        }
        self.state.depth -= 1;
        if self.shape.scoped() {
            self.state.opening = Some(b'{');
            Err(self.reject())
        } else {
            Ok(Value::Object(entries))
        }
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Value, A::Error> {
        self.state.scoped |= self.shape.scoped();
        self.state.enter("array")?;
        let shape = match self.shape {
            Shape::Any => Shape::Any,
            shape if shape.scoped() => Shape::Any,
            Shape::Collection(field) => Shape::element(field),
            _ => {
                self.state.opening = Some(b'[');
                return Err(self.reject());
            }
        };
        let mut values = Vec::new();
        let track_children = !matches!(self.shape, Shape::Any) && !self.shape.scoped();
        loop {
            if track_children {
                self.state.segments.push(format!("[{}]", values.len()));
            }
            self.state.awaiting_entry = true;
            let value = sequence.next_element_seed(Seed {
                shape,
                state: self.state,
            })?;
            if track_children {
                self.state.segments.pop();
            }
            let Some(value) = value else { break };
            values.push(value);
        }
        self.state.depth -= 1;
        if self.shape.scoped() {
            self.state.opening = Some(b'[');
            Err(self.reject())
        } else {
            Ok(Value::Array(values))
        }
    }
}
