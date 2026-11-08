//! 对齐 System.Text.Json 的默认与 UnsafeRelaxedJsonEscaping 编码器。
use std::{
    cell::Cell,
    io::{self, Write},
    rc::Rc,
};

use serde::Serialize;
use serde_json::ser::{CharEscape, CompactFormatter, Formatter};

pub(super) fn to_string<T: Serialize + ?Sized, const RELAXED: bool>(
    value: &T,
    raw_date: Rc<Cell<bool>>,
) -> Result<String, serde_json::Error> {
    let mut bytes = Vec::new();
    to_writer::<_, _, RELAXED>(&mut bytes, value, raw_date)?;
    String::from_utf8(bytes).map_err(serde::ser::Error::custom)
}

pub(super) fn to_writer<W: Write, T: Serialize + ?Sized, const RELAXED: bool>(
    writer: W,
    value: &T,
    raw_date: Rc<Cell<bool>>,
) -> Result<(), serde_json::Error> {
    to_writer_at_depth::<_, _, RELAXED>(writer, value, raw_date, 0)
}

pub(super) fn to_writer_at_depth<W: Write, T: Serialize + ?Sized, const RELAXED: bool>(
    writer: W,
    value: &T,
    raw_date: Rc<Cell<bool>>,
    depth: usize,
) -> Result<(), serde_json::Error> {
    value.serialize(&mut serde_json::Serializer::with_formatter(
        writer,
        JsonFormatter::<RELAXED> { raw_date, depth },
    ))
}

struct JsonFormatter<const RELAXED: bool> {
    raw_date: Rc<Cell<bool>>,
    depth: usize,
}

impl<const RELAXED: bool> JsonFormatter<RELAXED> {
    fn enter_container(&mut self) -> io::Result<()> {
        if self.depth >= super::MAX_DEPTH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "JSON 嵌套深度不能超过 64",
            ));
        }
        self.depth += 1;
        Ok(())
    }
}

impl<const RELAXED: bool> Formatter for JsonFormatter<RELAXED> {
    fn begin_array<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        self.enter_container()?;
        CompactFormatter.begin_array(writer)
    }

    fn end_array<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        self.depth -= 1;
        CompactFormatter.end_array(writer)
    }

    fn begin_object<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        self.enter_container()?;
        CompactFormatter.begin_object(writer)
    }

    fn end_object<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        self.depth -= 1;
        CompactFormatter.end_object(writer)
    }

    fn write_f32<W: Write + ?Sized>(&mut self, writer: &mut W, value: f32) -> io::Result<()> {
        let shortest = serde_json::to_string(&value).map_err(io::Error::other)?;
        write_float(writer, &shortest, 9)
    }

    fn write_f64<W: Write + ?Sized>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        let shortest = serde_json::to_string(&value).map_err(io::Error::other)?;
        write_float(writer, &shortest, 17)
    }

    fn write_string_fragment<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        // 原生 DateTimeOffset 写入器直接写已验证的 ISO ASCII，不转义时区正号。
        if self.raw_date.get() {
            return writer.write_all(fragment.as_bytes());
        }
        let mut start = 0;
        for (index, character) in fragment.char_indices() {
            if must_escape::<RELAXED>(character) {
                writer.write_all(&fragment.as_bytes()[start..index])?;
                let mut units = [0; 2];
                for unit in character.encode_utf16(&mut units) {
                    write_hex(writer, *unit)?;
                }
                start = index + character.len_utf8();
            }
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }

    fn write_char_escape<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        escape: CharEscape,
    ) -> io::Result<()> {
        match escape {
            CharEscape::Quote if !RELAXED => write_hex(writer, u16::from(b'"')),
            CharEscape::AsciiControl(value) => write_hex(writer, u16::from(value)),
            escape => CompactFormatter.write_char_escape(writer, escape),
        }
    }

    fn write_raw_fragment<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        // JsonElement 写回时字符串仍经过编码器。保留数字词法、键顺序和重复键，
        // 只逐个解码字符串并去除结构外空白，不能先转成 Value 丢失文档信息。
        let bytes = fragment.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if json_whitespace(bytes[index]) {
                index += 1;
                continue;
            }
            if bytes[index] == b'"' {
                let start = index;
                index += 1;
                loop {
                    match bytes.get(index) {
                        Some(b'"') => {
                            index += 1;
                            break;
                        }
                        Some(b'\\') => index += 2,
                        Some(_) => index += 1,
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "JSON 字符串未结束",
                            ));
                        }
                    }
                }
                let value: String =
                    serde_json::from_str(&fragment[start..index]).map_err(io::Error::other)?;
                to_writer::<_, _, RELAXED>(&mut *writer, &value, self.raw_date.clone())
                    .map_err(io::Error::other)?;
            } else {
                let start = index;
                while index < bytes.len() && bytes[index] != b'"' && !json_whitespace(bytes[index])
                {
                    // 原始文档不经过 begin_array/begin_object，必须计入当前位置的深度。
                    // 字符串已经在上面的分支完整跳过，转义文本中的括号不参与计数。
                    match bytes[index] {
                        b'[' | b'{' => self.enter_container()?,
                        b']' | b'}' => self.depth -= 1,
                        _ => {}
                    }
                    index += 1;
                }
                writer.write_all(&bytes[start..index])?;
            }
        }
        Ok(())
    }
}

fn json_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// 原生浮点值遵循 .NET 的默认 G 格式。最短可往返有效数字仍由 serde_json
/// 的浮点算法生成，只改变小数点位置和指数格式，不再次解析/舍入浮点值。
/// RawValue 与 arbitrary_precision 的 Number 走各自写入入口，不调用这里。
fn write_float<W: Write + ?Sized>(
    writer: &mut W,
    shortest: &str,
    max_digits: i32,
) -> io::Result<()> {
    let (negative, unsigned) = match shortest.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, shortest),
    };
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => {
            (mantissa, exponent.parse::<i32>().map_err(io::Error::other)?)
        }
        None => (unsigned, 0),
    };
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits = mantissa.replace('.', "");
    if digits.is_empty() || !digits.bytes().all(|value| value.is_ascii_digit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON 不支持非有限浮点数",
        ));
    }
    let significant = digits.trim_start_matches('0');
    let point = point - (digits.len() - significant.len()) as i32;
    let significant = significant.trim_end_matches('0').as_bytes();
    if negative {
        writer.write_all(b"-")?;
    }
    // 保留浮点负零，但不补 .0；不能套用 decimal 的负零规则。
    if significant.is_empty() {
        return writer.write_all(b"0");
    }
    if point > max_digits || point < -3 {
        writer.write_all(&significant[..1])?;
        if significant.len() > 1 {
            writer.write_all(b".")?;
            writer.write_all(&significant[1..])?;
        }
        write!(writer, "E{:+03}", point - 1)
    } else if point <= 0 {
        writer.write_all(b"0.")?;
        for _ in point..0 {
            writer.write_all(b"0")?;
        }
        writer.write_all(significant)
    } else {
        let point = point as usize;
        if point >= significant.len() {
            writer.write_all(significant)?;
            for _ in significant.len()..point {
                writer.write_all(b"0")?;
            }
            Ok(())
        } else {
            writer.write_all(&significant[..point])?;
            writer.write_all(b".")?;
            writer.write_all(&significant[point..])
        }
    }
}

fn must_escape<const RELAXED: bool>(value: char) -> bool {
    if !RELAXED && (!value.is_ascii() || matches!(value, '<' | '>' | '&' | '\'' | '+' | '`')) {
        return true;
    }
    let value = value as u32;
    if (0x20..0x7F).contains(&value) {
        return false;
    }
    if value > u32::from(u16::MAX) {
        return true;
    }
    let value = value as u16;
    let ranges = super::defined_bmp::BLOCKED;
    let index = ranges.partition_point(|(_, last)| *last < value);
    ranges.get(index).is_some_and(|(first, _)| *first <= value)
}

fn write_hex<W: Write + ?Sized>(writer: &mut W, value: u16) -> io::Result<()> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    writer.write_all(&[
        b'\\',
        b'u',
        HEX[usize::from(value >> 12)],
        HEX[usize::from((value >> 8) & 15)],
        HEX[usize::from((value >> 4) & 15)],
        HEX[usize::from(value & 15)],
    ])
}
