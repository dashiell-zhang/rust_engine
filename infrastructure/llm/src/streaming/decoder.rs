//! StreamReader(Encoding.UTF8) 的增量文本读取：保留读取边界、BOM 检测与解码器尾部。

const UTF8_PREAMBLE: &[u8] = b"\xef\xbb\xbf";
const READ_BUFFER_SIZE: usize = 1024;

pub(super) struct TextDecoder {
    encoding: Encoding,
    check_preamble: bool,
    detect_encoding: bool,
    preamble: Vec<u8>,
    pending: Vec<u8>,
}

impl Default for TextDecoder {
    fn default() -> Self {
        Self {
            encoding: Encoding::Utf8,
            check_preamble: true,
            detect_encoding: true,
            preamble: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl TextDecoder {
    pub(super) fn read_capacity(&self) -> usize {
        READ_BUFFER_SIZE - self.preamble.len()
    }

    pub(super) fn read(&mut self, bytes: &[u8]) -> String {
        let mut buffered = std::mem::take(&mut self.preamble);
        buffered.extend_from_slice(bytes);
        let mut bytes = buffered.as_slice();
        if self.check_preamble {
            let count = bytes.len().min(UTF8_PREAMBLE.len());
            if bytes[..count] == UTF8_PREAMBLE[..count] {
                if count < UTF8_PREAMBLE.len() {
                    self.preamble = buffered;
                    return String::new();
                }
                bytes = &bytes[UTF8_PREAMBLE.len()..];
                self.detect_encoding = false;
            }
            self.check_preamble = false;
        }
        if self.detect_encoding && bytes.len() >= 2 {
            if let Some((encoding, skipped)) = Encoding::detect(bytes) {
                self.encoding = encoding;
                // 源读取器切换编码时重建 decoder，不 flush 旧编码未完成的字符。
                self.pending.clear();
                bytes = &bytes[skipped..];
                self.detect_encoding = false;
            } else {
                self.detect_encoding = bytes.len() == 2;
            }
        }
        self.decode(bytes, false)
    }

    pub(super) fn finish(&mut self) -> String {
        let buffered = std::mem::take(&mut self.preamble);
        self.decode(&buffered, true)
    }

    fn decode(&mut self, bytes: &[u8], flush: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut count = self.pending.len();
        let mut output = match self.encoding {
            Encoding::Utf8 => {
                if !flush {
                    count = complete_utf8_prefix(&self.pending);
                }
                String::from_utf8_lossy(&self.pending[..count]).into_owned()
            }
            Encoding::Utf16 { little_endian } => {
                count -= count % 2;
                let unit = |pair: &[u8; 2]| {
                    if little_endian {
                        u16::from_le_bytes(*pair)
                    } else {
                        u16::from_be_bytes(*pair)
                    }
                };
                if !flush
                    && count >= 2
                    && (0xd800..=0xdbff)
                        .contains(&unit(&[self.pending[count - 2], self.pending[count - 1]]))
                {
                    count -= 2;
                }
                char::decode_utf16(self.pending[..count].as_chunks::<2>().0.iter().map(unit))
                    .map(|value| value.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect()
            }
            Encoding::Utf32 { little_endian } => {
                count -= count % 4;
                self.pending[..count]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|word| {
                        let value = if little_endian {
                            u32::from_le_bytes(*word)
                        } else {
                            u32::from_be_bytes(*word)
                        };
                        char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER)
                    })
                    .collect()
            }
        };
        self.pending.drain(..count);
        if flush && !self.pending.is_empty() {
            output.push(char::REPLACEMENT_CHARACTER);
            self.pending.clear();
        }
        output
    }
}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf16 { little_endian: bool },
    Utf32 { little_endian: bool },
}

impl Encoding {
    fn detect(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.starts_with(b"\xfe\xff") {
            Some((
                Self::Utf16 {
                    little_endian: false,
                },
                2,
            ))
        } else if bytes.starts_with(b"\xff\xfe\0\0") {
            Some((
                Self::Utf32 {
                    little_endian: true,
                },
                4,
            ))
        } else if bytes.starts_with(b"\xff\xfe") {
            Some((
                Self::Utf16 {
                    little_endian: true,
                },
                2,
            ))
        } else if bytes.starts_with(UTF8_PREAMBLE) {
            Some((Self::Utf8, 3))
        } else if bytes.starts_with(b"\0\0\xfe\xff") {
            Some((
                Self::Utf32 {
                    little_endian: false,
                },
                4,
            ))
        } else {
            None
        }
    }
}

fn complete_utf8_prefix(bytes: &[u8]) -> usize {
    let mut offset = 0;
    while let Err(error) = std::str::from_utf8(&bytes[offset..]) {
        offset += error.valid_up_to();
        match error.error_len() {
            Some(length) => offset += length,
            None => return offset,
        }
    }
    bytes.len()
}
