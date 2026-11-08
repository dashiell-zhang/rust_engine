//! MVC 对象结果的媒体类型选择；文件与直接响应不参与内容协商。
use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputKind {
    Json,
    String,
    Problem,
}

pub(crate) fn apply(mut response: Response, accept: &[HeaderValue]) -> Response {
    let Some(kind) = response.extensions().get::<OutputKind>().copied() else {
        return response;
    };
    if matches!(
        response.status(),
        StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
    ) || !response.headers().contains_key(header::CONTENT_TYPE)
    {
        return response;
    }
    let media = select(kind, accept);
    if kind == OutputKind::String && media != "text/plain; charset=utf-8" {
        // 使用格式选择前保留的对象快照，不再从响应正文重新收集字符串。
        let Some(object) = response
            .extensions()
            .get::<crate::object_response::ObjectResponse>()
        else {
            return crate::error::internal_error_response("字符串响应缺少对象快照");
        };
        let json = object.json.clone();
        response.headers_mut().remove(header::CONTENT_LENGTH);
        *response.body_mut() = Body::from(json);
    }
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(media));
    response
}

fn select(kind: OutputKind, headers: &[HeaderValue]) -> &'static str {
    // ProblemDetails 声明了服务端内容类型；源宿主只有 JSON 格式化器。
    if kind == OutputKind::Problem {
        return "application/problem+json; charset=utf-8";
    }
    let fallback = if kind == OutputKind::String {
        "text/plain; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    let mut values = Vec::new();
    for header in headers {
        if let Ok(value) = header.to_str() {
            parse_accept(value, &mut values);
        }
    }
    // RespectBrowserAcceptHeader=false：任意有效 */*（包括 q=0）使整组 Accept 被忽略。
    if values
        .iter()
        .any(|value| value.main == "*" && value.sub == "*")
    {
        return fallback;
    }
    if values.len() > 1 {
        let depth = 2 * (values.len().ilog2() + 1);
        sort_accept(&mut values, depth);
    }
    for value in values {
        // 源选择器保留 q=0，不添加特异度排序，也不回显客户端任意 vendor+json 类型。
        if kind == OutputKind::String && value.matches("text", "plain") {
            return "text/plain; charset=utf-8";
        }
        if value.matches("application", "json") {
            return "application/json; charset=utf-8";
        }
        if value.matches("text", "json") {
            return "text/json; charset=utf-8";
        }
    }
    // ReturnHttpNotAcceptable=false：无法匹配时使用该结果的默认格式化器。
    fallback
}

#[derive(Clone, Copy)]
struct Accept<'a> {
    main: &'a str,
    sub: &'a str,
    quality: f64,
    has_required_parameters: bool,
}

impl Accept<'_> {
    fn matches(&self, main: &str, sub: &str) -> bool {
        // 当前普通格式化器的具体支持类型没有后缀或参数；服务端 ProblemDetails 单独处理。
        !self.has_required_parameters
            && (self.main == "*" || self.main.eq_ignore_ascii_case(main))
            && (self.sub == "*" || self.sub.eq_ignore_ascii_case(sub))
    }
}

fn parse_accept<'a>(input: &'a str, output: &mut Vec<Accept<'a>>) {
    let mut rest = input;
    while !rest.is_empty() {
        rest = rest.trim_start_matches([' ', '\t', ',']);
        if rest.is_empty() {
            break;
        }
        match parse_media(rest) {
            Some((value, tail)) => {
                let tail = tail.trim_start_matches([' ', '\t']);
                if tail.is_empty() || tail.starts_with(',') {
                    output.push(value);
                }
                // 合法前缀后缺逗号时，从未消费的位置继续恢复。
                rest = tail;
            }
            None => {
                // 无法解析的媒体类型或质量跳至下一个逗号，不否定其他条目。
                rest = rest.find(',').map_or("", |index| &rest[index + 1..]);
            }
        }
    }
}

fn parse_media(input: &str) -> Option<(Accept<'_>, &str)> {
    let (main, rest) = take_token(input)?;
    let rest = rest.trim_start_matches([' ', '\t']).strip_prefix('/')?;
    let (sub, rest) = take_token(rest.trim_start_matches([' ', '\t']))?;
    let mut rest = rest.trim_start_matches([' ', '\t']);
    let mut value = Accept {
        main,
        sub,
        quality: 1.0,
        has_required_parameters: false,
    };
    let mut after_quality = false;
    while let Some(parameter) = rest.strip_prefix(';') {
        let Some((name, tail)) = take_token(parameter.trim_start_matches([' ', '\t'])) else {
            break;
        };
        let tail = tail.trim_start_matches([' ', '\t']);
        if tail.is_empty() && name == "*" {
            rest = tail;
            break;
        }
        let Some(tail) = tail.strip_prefix('=') else {
            break;
        };
        let (parameter, tail) = take_value(tail.trim_start_matches([' ', '\t']))?;
        if name.eq_ignore_ascii_case("q") {
            // 源使用 AllowDecimalPoint，重复 q 取最后值，不限制到 [0, 1]。
            value.quality = parse_quality(parameter)?;
            after_quality = true;
        } else if !after_quality && name != "*" {
            value.has_required_parameters = true;
        }
        rest = tail.trim_start_matches([' ', '\t']);
    }
    Some((value, rest))
}

fn take_token(value: &str) -> Option<(&str, &str)> {
    let length = value
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte))
        .count();
    (length > 0).then(|| value.split_at(length))
}

fn parse_quality(value: &str) -> Option<f64> {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return value.parse().ok();
    }
    // double.Parse 的特殊浮点符号不受 AllowDecimalPoint 限制。
    let value = value.trim();
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    if unsigned.eq_ignore_ascii_case("nan") {
        Some(f64::NAN)
    } else if unsigned.eq_ignore_ascii_case("infinity") {
        Some(if value.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    } else {
        None
    }
}

fn quality_order(left: Accept<'_>, right: Accept<'_>) -> std::cmp::Ordering {
    // 保留源选择器的比较规则，包括 NaN；不使用会改变这些值顺序的 total_cmp。
    if left.quality > right.quality {
        std::cmp::Ordering::Less
    } else if left.quality == right.quality {
        std::cmp::Ordering::Equal
    } else {
        std::cmp::Ordering::Greater
    }
}

/// 对齐 List.Sort 的分区与小数组分支；超过 16 项时同质量项并不保持原顺序。
fn sort_accept(mut values: &mut [Accept<'_>], mut depth: u32) {
    while values.len() > 1 {
        let last = values.len() - 1;
        if values.len() == 2 {
            swap_out_of_order(values, 0, 1);
            return;
        }
        if values.len() == 3 {
            for (left, right) in [(0, 1), (0, last), (1, last)] {
                swap_out_of_order(values, left, right);
            }
            return;
        }
        if values.len() <= 16 {
            for next in 1..values.len() {
                let item = values[next];
                let mut position = next;
                while position > 0 && quality_order(item, values[position - 1]).is_lt() {
                    values[position] = values[position - 1];
                    position -= 1;
                }
                values[position] = item;
            }
            return;
        }
        if depth == 0 {
            for root in (1..=values.len() / 2).rev() {
                down_heap(values, root);
            }
            for end in (1..values.len()).rev() {
                values.swap(0, end);
                down_heap(&mut values[..end], 1);
            }
            return;
        }
        depth -= 1;
        let middle = last / 2;
        for (left, right) in [(0, middle), (0, last), (middle, last)] {
            swap_out_of_order(values, left, right);
        }
        let pivot = values[middle];
        values.swap(middle, last - 1);
        let (mut left, mut right) = (0, last - 1);
        loop {
            left += 1;
            while quality_order(values[left], pivot).is_lt() {
                left += 1;
            }
            right -= 1;
            while quality_order(pivot, values[right]).is_lt() {
                right -= 1;
            }
            if left >= right {
                break;
            }
            values.swap(left, right);
        }
        values.swap(left, last - 1);
        sort_accept(&mut values[left + 1..], depth);
        values = &mut values[..left];
    }
}

fn swap_out_of_order(values: &mut [Accept<'_>], left: usize, right: usize) {
    if left != right && quality_order(values[left], values[right]).is_gt() {
        values.swap(left, right);
    }
}

fn down_heap(values: &mut [Accept<'_>], mut root: usize) {
    let value = values[root - 1];
    while root <= values.len() / 2 {
        let mut child = root * 2;
        if child < values.len() && quality_order(values[child - 1], values[child]).is_lt() {
            child += 1;
        }
        if !quality_order(value, values[child - 1]).is_lt() {
            break;
        }
        values[root - 1] = values[child - 1];
        root = child;
    }
    values[root - 1] = value;
}

fn take_value(value: &str) -> Option<(&str, &str)> {
    if let Some(quoted) = value.strip_prefix('"') {
        let mut escaped = false;
        for (index, byte) in quoted.bytes().enumerate() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return Some((&quoted[..index], &quoted[index + 1..]));
            }
        }
        None
    } else {
        take_token(value)
    }
}
