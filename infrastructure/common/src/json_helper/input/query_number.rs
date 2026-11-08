use std::{borrow::Cow, str::FromStr};

/// Query 使用对应 .NET 模型绑定器的数字规则，JSON 数字字符串不复用此规则。
pub(super) trait QueryNumber: FromStr {
    const ALLOW_NON_FINITE: bool = false;

    fn parse_query(value: &str) -> Option<Self> {
        decimal_text(value).parse().ok()
    }
}

macro_rules! integer {
    ($number:ty, $unsigned:ty) => {
        impl QueryNumber for $number {
            fn parse_query(value: &str) -> Option<Self> {
                // 整数 TypeConverter 在进制判定之前调用 String.Trim。
                let value = value.trim();
                if let Some(hex) = hex_digits(value) {
                    // 非十进制允许设置符号位，但不得超过目标整数自身的位宽。
                    if hex.is_empty() || !hex.bytes().all(|value| value.is_ascii_hexdigit()) {
                        return None;
                    }
                    return <$unsigned>::from_str_radix(hex, 16)
                        .ok()
                        .map(|value| value as Self);
                }
                let value = decimal_text(value);
                if <$number>::MIN == 0
                    && let Some(digits) = value.strip_prefix('-')
                    && !digits.is_empty()
                    && digits.bytes().all(|value| value == b'0')
                {
                    return Some(0);
                }
                value.parse().ok()
            }
        }
    };
}

integer!(i8, u8);
integer!(i16, u16);
integer!(i32, u32);
integer!(i64, u64);
integer!(u8, u8);
integer!(u16, u16);
integer!(u32, u32);
integer!(u64, u64);
impl QueryNumber for i128 {}
impl QueryNumber for u128 {}

macro_rules! floating_point {
    ($number:ty) => {
        impl QueryNumber for $number {
            const ALLOW_NON_FINITE: bool = true;

            fn parse_query(value: &str) -> Option<Self> {
                // 浮点绑定器直接使用 Float | AllowThousands，不经过整数的 Trim/进制转换。
                let special = value.trim();
                let unsigned = special.strip_prefix(['+', '-']).unwrap_or(special);
                if unsigned.eq_ignore_ascii_case("NaN") {
                    return Some(Self::NAN);
                }
                if unsigned.eq_ignore_ascii_case("Infinity") {
                    return Some(if special.starts_with('-') {
                        Self::NEG_INFINITY
                    } else {
                        Self::INFINITY
                    });
                }
                normalize_float(value)?.parse().ok()
            }
        }
    };
}

floating_point!(f32);
floating_point!(f64);

fn number_whitespace(value: char) -> bool {
    matches!(value, '\u{0009}'..='\u{000d}' | ' ')
}

/// 数字解析只允许尾部空白之后的连续 NUL，不允许开头 NUL 或 NUL 后再跟空白。
fn decimal_text(value: &str) -> &str {
    value.trim_end_matches('\0').trim_matches(number_whitespace)
}

fn normalize_float(value: &str) -> Option<Cow<'_, str>> {
    let value = decimal_text(value);
    let mut has_digit = false;
    let mut allows_group = true;
    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' => has_digit = true,
            b',' if has_digit && allows_group => {}
            b'.' | b'e' | b'E' => allows_group = false,
            b'+' | b'-' => {}
            // 排除 Rust 接受、但 invariant .NET 不接受的 inf，以及数字内部的空白。
            _ => return None,
        }
    }
    // .NET 不校验千位分组宽度；逗号仅能出现在已有整数数字之后、小数点/指数之前。
    Some(if value.contains(',') {
        Cow::Owned(value.replace(',', ""))
    } else {
        Cow::Borrowed(value)
    })
}

fn hex_digits(value: &str) -> Option<&str> {
    let value = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))
        .or_else(|| value.strip_prefix("0X"))
        .or_else(|| value.strip_prefix("&h"))
        .or_else(|| value.strip_prefix("&H"))?;
    // BaseNumberConverter 去除外层前缀后，Convert 的 radix=16 解析还接受 + 和 0x。
    let value = value.strip_prefix('+').unwrap_or(value);
    Some(
        value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
            .unwrap_or(value),
    )
}
