use std::{iter::Peekable, str::EncodeUtf16, sync::LazyLock};

use regex::Regex;

// RegexCharClass.IsBoundaryWordChar 的类别，不使用范围更广的 Rust \w。
static BOUNDARY_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\p{L}\p{Mn}\p{Nd}\p{Pc}\x{200c}\x{200d}]").expect("转义字符类别正则应有效")
});

/// 按 Regex.Unescape 解码；非法转义或不能表示为 Rust String 的孤立代理项返回错误。
pub fn unicode_to_string(text: &str) -> Result<String, String> {
    let units = unicode_to_utf16(text)?;
    String::from_utf16(&units).map_err(|_| "转义结果含有孤立 UTF-16 代理项".to_owned())
}

/// 保留 .NET 字符串的 UTF-16 结果，包括合法转义产生的孤立代理项。
pub fn unicode_to_utf16(text: &str) -> Result<Vec<u16>, String> {
    let mut input = text.encode_utf16().peekable();
    let mut output = Vec::with_capacity(text.len());
    while let Some(unit) = input.next() {
        if unit == u16::from(b'\\') {
            // Regex.Unescape 忽略末尾单独的反斜杠。
            if let Some(escaped) = input.next() {
                output.push(scan_escape(escaped, &mut input)?);
            }
        } else {
            output.push(unit);
        }
    }
    Ok(output)
}

fn scan_escape(unit: u16, input: &mut Peekable<EncodeUtf16<'_>>) -> Result<u16, String> {
    let Some(ch) = char::from_u32(u32::from(unit)) else {
        return Ok(unit);
    };
    Ok(match ch {
        '0'..='7' => {
            let mut value = unit - u16::from(b'0');
            for _ in 0..2 {
                let Some(next) = input.next_if(|unit| (0x30..=0x37).contains(unit)) else {
                    break;
                };
                value = value * 8 + next - u16::from(b'0');
            }
            value & 0xff
        }
        'x' => scan_hex(input, 2)?,
        'u' => scan_hex(input, 4)?,
        'a' => 0x07,
        'b' => 0x08,
        'e' => 0x1b,
        'f' => 0x0c,
        'n' => 0x0a,
        'r' => 0x0d,
        't' => 0x09,
        'v' => 0x0b,
        'c' => {
            let mut control = input.next().ok_or("缺少控制转义字符")?;
            if (0x61..=0x7a).contains(&control) {
                control -= 0x20;
            }
            control
                .checked_sub(0x40)
                .filter(|value| *value < 0x20)
                .ok_or("无效的控制转义字符")?
        }
        _ => {
            if BOUNDARY_WORD.is_match(ch.encode_utf8(&mut [0; 4])) {
                return Err("无法识别的转义字符".to_owned());
            }
            unit
        }
    })
}

fn scan_hex(input: &mut Peekable<EncodeUtf16<'_>>, count: usize) -> Result<u16, String> {
    let mut value = 0u16;
    for _ in 0..count {
        let digit = input
            .next()
            .and_then(|unit| char::from_u32(u32::from(unit)))
            .and_then(|ch| ch.to_digit(16))
            .ok_or("十六进制转义位数不足或包含非法字符")?;
        value = (value << 4) | digit as u16;
    }
    Ok(value)
}
