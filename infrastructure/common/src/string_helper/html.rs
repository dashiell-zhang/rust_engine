use std::sync::LazyLock;

use regex::Regex;

use super::html_entities;

// 保留源 RemoveHtml 的顺序；zh-CN 的 IgnoreCase 对 I 还包含 U+0130，
// 但不会把 long s 当作 s。不能直接使用 Rust 的 Unicode case folding。
static CLEANUP: LazyLock<[(Regex, &'static str); 15]> = LazyLock::new(|| {
    [
        (
            r"<[sS][cC][rR][iI\x{130}][pP][tT][^>]*?>[\s\S]*?</[sS][cC][rR][iI\x{130}][pP][tT]>",
            "",
        ),
        (r"<(.[^>]*)>", ""),
        (r"([\r\n])[\s]+", ""),
        (r"-->", ""),
        (r"<!--.*", ""),
        (r"&([qQ][uU][oO][tT]|#34);", "\""),
        (r"&([aA][mM][pP]|#38);", "&"),
        (r"&([lL][tT]|#60);", "<"),
        (r"&([gG][tT]|#62);", ">"),
        (r"&([nN][bB][sS][pP]|#160);", " "),
        (r"&([iI\x{130}][eE][xX][cC][lL]|#161);", "\u{a1}"),
        (r"&([cC][eE][nN][tT]|#162);", "\u{a2}"),
        (r"&([pP][oO][uU][nN][dD]|#163);", "\u{a3}"),
        (r"&([cC][oO][pP][yY]|#169);", "\u{a9}"),
        (r"&#([\p{Nd}&&[\x{0}-\x{FFFF}]]+);", ""),
    ]
    .map(|(pattern, replacement)| {
        (
            Regex::new(pattern).expect("HTML 清理正则应有效"),
            replacement,
        )
    })
});

/// 按源助手清理 HTML 文本，不合并正文空白，也不用于过滤可执行 HTML。
pub fn remove_html(html_text: &str) -> String {
    let mut text = decode(html_text);
    for (pattern, replacement) in CLEANUP.iter() {
        if let std::borrow::Cow::Owned(cleaned) = pattern.replace_all(&text, *replacement) {
            text = cleaned;
        }
    }
    text.replace(['<', '>'], "")
        .replace("\r\n", "")
        .trim()
        .to_owned()
}

// WebUtility.HtmlDecode 只接受有分号的 HTML4 实体加 apos；实体不递归解码。
fn decode(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start + 1..];
        if let Some(end) = remaining.find([';', '&'])
            && remaining.as_bytes()[end] == b';'
        {
            let entity = &remaining[..end];
            let decoded = if let Some(number) = entity.strip_prefix('#') {
                decode_number(number)
            } else {
                html_entities::lookup(entity)
            };
            if let Some(decoded) = decoded {
                output.push(decoded);
            } else {
                output.push('&');
                output.push_str(&remaining[..=end]);
            }
            remaining = &remaining[end + 1..];
        } else {
            output.push('&');
        }
    }
    output.push_str(remaining);
    output
}

fn decode_number(text: &str) -> Option<char> {
    let text = text.trim_end_matches('\0');
    let (digits, radix, negative) = if let Some(hex) = text.strip_prefix(['x', 'X']) {
        (hex, 16u32, false)
    } else {
        // NumberStyles.Integer 只接受 ASCII 数字空白；无符号解析允许负零。
        let text = text.trim_matches(|ch| matches!(ch, ' ' | '\t'..='\r'));
        let negative = text.starts_with('-');
        let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
        (digits, 10u32, negative)
    };
    if digits.is_empty() {
        return None;
    }
    let value = digits.bytes().try_fold(0u32, |value, byte| {
        let digit = char::from(byte).to_digit(radix)?;
        value.checked_mul(radix)?.checked_add(digit)
    })?;
    if negative && value != 0 {
        return None;
    }
    char::from_u32(value)
}
