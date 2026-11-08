use std::io::{self, ErrorKind, Read};

use base64::{Engine as _, engine::general_purpose};
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use super::{decode_base64, text::read_bom_text};

/// UTF-8 文本压缩为 gzip 后返回 Base64，不添加文本 BOM。
pub fn compress_string(text: &str) -> io::Result<String> {
    let mut compressor = Compress::new_gzip(Compression::default(), 15);
    let mut remaining = text.as_bytes();
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let before_in = compressor.total_in();
        let before_out = compressor.total_out();
        let flush = if remaining.is_empty() {
            FlushCompress::Finish
        } else {
            FlushCompress::None
        };
        let status = compressor
            .compress(remaining, &mut buffer, flush)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let consumed = (compressor.total_in() - before_in) as usize;
        let written = (compressor.total_out() - before_out) as usize;
        remaining = &remaining[consumed..];
        output.extend_from_slice(&buffer[..written]);
        if status == Status::StreamEnd {
            return Ok(general_purpose::STANDARD.encode(output));
        }
        if consumed == 0 && written == 0 {
            return Err(io::Error::other("gzip 压缩未取得进展"));
        }
    }
}

/// Base64/gzip 解码后按 BOM 读取文本，非法文本字节使用替换字符。
pub fn decompress_string(text: &str) -> io::Result<String> {
    let compressed =
        decode_base64(text).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    read_bom_text(GzipReader::new(&compressed))
}

// 源 DeflateStream 每次最多补充 8192 字节，读出非空数据立即返回给 StreamReader。
// 保留此边界，避免相邻 gzip 段和分开的 BOM 在合并后被重新解释。
struct GzipReader<'a> {
    decoder: Decompress,
    source: &'a [u8],
    input: &'a [u8],
    member_ended: bool,
    finished: bool,
}

impl<'a> GzipReader<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self {
            decoder: Decompress::new_gzip(15),
            source,
            input: &[],
            member_ended: false,
            finished: false,
        }
    }

    fn fill_input(&mut self) {
        let (input, remaining) = self.source.split_at(self.source.len().min(8192));
        self.input = input;
        self.source = remaining;
    }
}

impl Read for GzipReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.finished || buffer.is_empty() {
            return Ok(0);
        }
        loop {
            if self.member_ended {
                if self.input.is_empty() {
                    self.fill_input();
                }
                // 源 GZipStream 忽略完整段之后的非 gzip 尾部；单独的 1f 仍尝试下一段。
                if self.input.first() != Some(&0x1f)
                    || self.input.get(1).is_some_and(|byte| *byte != 0x8b)
                {
                    self.finished = true;
                    return Ok(0);
                }
                self.decoder.reset(true);
                self.member_ended = false;
            }
            let before_in = self.decoder.total_in();
            let before_out = self.decoder.total_out();
            let status = self
                .decoder
                .decompress(self.input, buffer, FlushDecompress::None)
                .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;
            let consumed = (self.decoder.total_in() - before_in) as usize;
            let written = (self.decoder.total_out() - before_out) as usize;
            self.input = &self.input[consumed..];
            self.member_ended = status == Status::StreamEnd;
            if written != 0 {
                return Ok(written);
            }
            if self.member_ended {
                continue;
            }
            if self.input.is_empty() {
                self.fill_input();
                if self.input.is_empty() {
                    // .NET 默认未启用 UseStrictValidation，EOF 允许返回已解出的部分。
                    self.finished = true;
                    return Ok(0);
                }
            } else if consumed == 0 {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "gzip 解压未取得进展",
                ));
            }
        }
    }
}
