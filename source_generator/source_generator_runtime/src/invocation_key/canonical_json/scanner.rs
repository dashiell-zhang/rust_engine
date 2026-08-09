use crate::*;

/// 在构建 JSON 值树之前按原始 JSON 语法检查深度和节点数量
pub(crate) fn validate_json_complexity(
    bytes: &[u8],

    max_depth: usize,

    max_items: usize,
) -> Result<(), InvocationKeyError> {
    JsonComplexityScanner {
        bytes,

        index: 0,

        item_count: 0,

        max_depth,

        max_items,
    }
    .validate()
}

/// 原始 JSON 复杂度扫描器
pub(crate) struct JsonComplexityScanner<'a> {
    bytes: &'a [u8],

    index: usize,

    item_count: usize,

    max_depth: usize,

    max_items: usize,
}

impl JsonComplexityScanner<'_> {
    /// 扫描唯一根值并确认没有额外内容
    fn validate(mut self) -> Result<(), InvocationKeyError> {
        self.skip_whitespace();

        self.parse_value(0)?;

        self.skip_whitespace();

        if self.index != self.bytes.len() {
            return Err(self.error("参数 JSON 包含额外内容"));
        }

        Ok(())
    }

    /// 扫描一个 JSON 值并计入真实语法节点
    fn parse_value(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.consume_item(depth)?;

        self.skip_whitespace();

        match self.bytes.get(self.index).copied() {
            Some(b'n') => self.parse_literal(b"null"),

            Some(b't') => self.parse_literal(b"true"),

            Some(b'f') => self.parse_literal(b"false"),

            Some(b'"') => self.parse_string(),

            Some(b'[') => self.parse_array(depth),

            Some(b'{') => self.parse_object(depth),

            Some(b'-' | b'0'..=b'9') => self.parse_number(),

            Some(_) => Err(self.error("参数 JSON 包含无效值")),

            None => Err(self.error("参数 JSON 意外结束")),
        }
    }

    /// 消耗一个节点的深度与数量预算
    fn consume_item(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        if depth > self.max_depth {
            return Err(InvocationKeyError::new(format!(
                "参数 JSON 超过 {} 层深度限制",
                self.max_depth
            )));
        }

        self.item_count = self
            .item_count
            .checked_add(1)
            .ok_or_else(|| InvocationKeyError::new("参数 JSON 项目计数溢出"))?;

        if self.item_count > self.max_items {
            return Err(InvocationKeyError::new(format!(
                "参数 JSON 超过 {} 项限制",
                self.max_items
            )));
        }

        Ok(())
    }

    /// 扫描固定 JSON 字面量
    fn parse_literal(&mut self, literal: &[u8]) -> Result<(), InvocationKeyError> {
        if self.bytes.get(self.index..self.index + literal.len()) != Some(literal) {
            return Err(self.error("参数 JSON 包含无效字面量"));
        }

        self.index += literal.len();

        Ok(())
    }

    /// 扫描 JSON 字符串并校验转义序列
    fn parse_string(&mut self) -> Result<(), InvocationKeyError> {
        self.index += 1;

        while let Some(byte) = self.bytes.get(self.index).copied() {
            match byte {
                b'"' => {
                    self.index += 1;

                    return Ok(());
                }

                b'\\' => {
                    self.index += 1;

                    match self.bytes.get(self.index).copied() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.index += 1;
                        }

                        Some(b'u') => {
                            self.index += 1;

                            let Some(hex) = self.bytes.get(self.index..self.index + 4) else {
                                return Err(self.error("参数 JSON Unicode 转义不完整"));
                            };

                            if !hex.iter().all(u8::is_ascii_hexdigit) {
                                return Err(self.error("参数 JSON Unicode 转义无效"));
                            }

                            self.index += 4;
                        }

                        _ => return Err(self.error("参数 JSON 字符串转义无效")),
                    }
                }

                0x00..=0x1f => return Err(self.error("参数 JSON 字符串包含控制字符")),

                _ => self.index += 1,
            }
        }

        Err(self.error("参数 JSON 字符串未结束"))
    }

    /// 扫描 JSON 数组
    fn parse_array(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.index += 1;

        self.skip_whitespace();

        if self.consume_byte(b']') {
            return Ok(());
        }

        loop {
            self.parse_value(depth.saturating_add(1))?;

            self.skip_whitespace();

            if self.consume_byte(b']') {
                return Ok(());
            }

            if !self.consume_byte(b',') {
                return Err(self.error("参数 JSON 数组缺少逗号或结束符"));
            }

            self.skip_whitespace();
        }
    }

    /// 扫描 JSON 对象，属性名不计入 JSON 值节点
    fn parse_object(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.index += 1;

        self.skip_whitespace();

        if self.consume_byte(b'}') {
            return Ok(());
        }

        let mut property_names = HashSet::new();

        loop {
            if self.bytes.get(self.index) != Some(&b'"') {
                return Err(self.error("参数 JSON 对象属性名必须是字符串"));
            }

            let property_start = self.index;

            self.parse_string()?;

            let property_end = self.index;

            let property_name: String = serde_json::from_slice(
                self.bytes
                    .get(property_start..property_end)
                    .ok_or_else(|| self.error("参数 JSON 对象属性名范围无效"))?,
            )
            .map_err(|error| self.error(&format!("参数 JSON 对象属性名无法解析: {error}")))?;

            if !property_names.insert(property_name.clone()) {
                let display_name = if property_name.chars().count() > 128 {
                    format!("{}…", property_name.chars().take(128).collect::<String>())
                } else {
                    property_name
                };

                return Err(self.error(&format!("参数 JSON 对象包含重复属性 {display_name:?}")));
            }

            self.skip_whitespace();

            if !self.consume_byte(b':') {
                return Err(self.error("参数 JSON 对象属性名后缺少冒号"));
            }

            self.skip_whitespace();

            self.parse_value(depth.saturating_add(1))?;

            self.skip_whitespace();

            if self.consume_byte(b'}') {
                return Ok(());
            }

            if !self.consume_byte(b',') {
                return Err(self.error("参数 JSON 对象缺少逗号或结束符"));
            }

            self.skip_whitespace();
        }
    }

    /// 扫描符合 JSON 语法的数字文本
    fn parse_number(&mut self) -> Result<(), InvocationKeyError> {
        self.consume_byte(b'-');

        match self.bytes.get(self.index).copied() {
            Some(b'0') => self.index += 1,

            Some(b'1'..=b'9') => {
                self.index += 1;

                self.consume_digits();
            }

            _ => return Err(self.error("参数 JSON 数字整数部分无效")),
        }

        if self.consume_byte(b'.') {
            if !self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("参数 JSON 数字小数部分无效"));
            }

            self.consume_digits();
        }

        if self
            .bytes
            .get(self.index)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.index += 1;

            if self
                .bytes
                .get(self.index)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.index += 1;
            }

            if !self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("参数 JSON 数字指数部分无效"));
            }

            self.consume_digits();
        }

        Ok(())
    }

    /// 消耗连续十进制数字
    fn consume_digits(&mut self) {
        while self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
    }

    /// 消耗指定字节
    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.index) != Some(&expected) {
            return false;
        }

        self.index += 1;

        true
    }

    /// 跳过 JSON 允许的空白字符
    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.index += 1;
        }
    }

    /// 创建带字节偏移的 JSON 扫描错误
    fn error(&self, message: &str) -> InvocationKeyError {
        InvocationKeyError::new(format!("{message}，位置 {}", self.index))
    }
}
