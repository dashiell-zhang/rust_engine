use std::io::{self, Read};

/// 按 UTF-16 单元截断并添加省略号；负长度或孤立代理项不能转换为 String。
pub fn substring_text(text: &str, length: i32) -> Result<String, String> {
    let units = text.encode_utf16().collect::<Vec<_>>();
    let truncated = substring_text_utf16(&units, length)?;
    String::from_utf16(&truncated).map_err(|_| "截断结果含有孤立 UTF-16 代理项".to_owned())
}

/// 保留源 String 截断结果，包括恰好分开代理项对的情况。
pub fn substring_text_utf16(text: &[u16], length: i32) -> Result<Vec<u16>, String> {
    let length = usize::try_from(length).map_err(|_| "截断长度不能为负数")?;
    if text.len() <= length {
        return Ok(text.to_vec());
    }
    let mut output = text[..length].to_vec();
    output.extend([u16::from(b'.'); 3]);
    Ok(output)
}

/// 按 UTF-16 长度隐藏中间三分之一；不能表示为 String 的结果返回错误。
pub fn partially_hidden(text: &str) -> Result<String, String> {
    let units = text.encode_utf16().collect::<Vec<_>>();
    String::from_utf16(&partially_hidden_utf16(&units))
        .map_err(|_| "隐藏结果含有孤立 UTF-16 代理项".to_owned())
}

/// 保留原始 UTF-16 隐藏结果；一个单元全部隐藏，两个单元仅隐藏第二个。
pub fn partially_hidden_utf16(text: &[u16]) -> Vec<u16> {
    let mut output = text.to_vec();
    let range = match output.len() {
        0 => return output,
        1 => 0..1,
        2 => 1..2,
        length => {
            let group = length / 3;
            group..group * 2
        }
    };
    output[range].fill(u16::from(b'*'));
    output
}

/// 缓冲文本沿用 StreamReader 的 BOM 识别，无 BOM 时使用 UTF-8 替换解码。
pub fn decode_bom_text(bytes: &[u8]) -> String {
    let (encoding, skipped) = detect_encoding(bytes).unwrap_or((TextEncoding::Utf8, 0));
    encoding.decode(&bytes[skipped..])
}

// StreamReader(Encoding.UTF8).ReadToEnd 使用 1024 字节输入缓冲。
// 未完成的 UTF-8 preamble 会继续读取；其他 BOM 则按当次可见字节立即判定。
pub(super) fn read_bom_text(mut reader: impl Read) -> io::Result<String> {
    const PREAMBLE: [u8; 3] = [0xef, 0xbb, 0xbf];
    let mut buffer = [0u8; 1024];
    let mut buffered = 0;
    let mut check_preamble = true;
    let mut detect = true;
    let mut encoding = TextEncoding::Utf8;
    let mut segment = Vec::new();
    let mut output = String::new();
    loop {
        let count = reader.read(&mut buffer[buffered..])?;
        if count == 0 {
            segment.extend_from_slice(&buffer[..buffered]);
            break;
        }
        let count = buffered + count;
        buffered = 0;
        let mut bytes = &buffer[..count];
        if check_preamble {
            let matched = count.min(PREAMBLE.len());
            if bytes[..matched] == PREAMBLE[..matched] {
                if count < PREAMBLE.len() {
                    buffered = count;
                    continue;
                }
                bytes = &bytes[PREAMBLE.len()..];
                detect = false;
            }
            check_preamble = false;
        }
        if detect && bytes.len() >= 2 {
            if let Some((selected, skipped)) = detect_encoding(bytes) {
                // 切换编码会重建 decoder；此前未完成的 UTF-8 序列不再 flush。
                output.push_str(&decode_utf8_prefix(&segment));
                segment.clear();
                encoding = selected;
                bytes = &bytes[skipped..];
                detect = false;
            } else {
                detect = bytes.len() == 2;
            }
        }
        segment.extend_from_slice(bytes);
    }
    output.push_str(&encoding.decode(&segment));
    Ok(output)
}

fn decode_utf8_prefix(bytes: &[u8]) -> String {
    let mut remaining = bytes;
    while let Err(error) = std::str::from_utf8(remaining) {
        let Some(length) = error.error_len() else {
            let end = bytes.len() - remaining.len() + error.valid_up_to();
            return String::from_utf8_lossy(&bytes[..end]).into_owned();
        };
        remaining = &remaining[error.valid_up_to() + length..];
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn detect_encoding(bytes: &[u8]) -> Option<(TextEncoding, usize)> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        Some((TextEncoding::Utf8, 3))
    } else if bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
        Some((TextEncoding::Utf32(true), 4))
    } else if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        Some((TextEncoding::Utf32(false), 4))
    } else if bytes.starts_with(&[0xff, 0xfe]) {
        Some((TextEncoding::Utf16(true), 2))
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        Some((TextEncoding::Utf16(false), 2))
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum TextEncoding {
    Utf8,
    Utf16(bool),
    Utf32(bool),
}

impl TextEncoding {
    fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Self::Utf16(little_endian) => {
                let (chunks, remainder) = bytes.as_chunks::<2>();
                let words = chunks.iter().map(|chunk| {
                    if little_endian {
                        u16::from_le_bytes(*chunk)
                    } else {
                        u16::from_be_bytes(*chunk)
                    }
                });
                let mut output: String = char::decode_utf16(words)
                    .map(|value| value.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect();
                if !remainder.is_empty() {
                    output.push(char::REPLACEMENT_CHARACTER);
                }
                output
            }
            Self::Utf32(little_endian) => {
                let (chunks, remainder) = bytes.as_chunks::<4>();
                let mut output: String = chunks
                    .iter()
                    .map(|chunk| {
                        let value = if little_endian {
                            u32::from_le_bytes(*chunk)
                        } else {
                            u32::from_be_bytes(*chunk)
                        };
                        char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER)
                    })
                    .collect();
                if !remainder.is_empty() {
                    output.push(char::REPLACEMENT_CHARACTER);
                }
                output
            }
        }
    }
}
