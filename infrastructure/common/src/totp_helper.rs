use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

/// 支持的哈希算法类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    /// SHA1
    Sha1,

    /// SHA256
    Sha256,

    /// SHA512
    Sha512,
}

/// 默认密钥长度
pub const DEFAULT_SECRET_BYTES: usize = 20;

/// 默认验证码位数
pub const DEFAULT_DIGITS: u32 = 6;

/// 默认时间步长
pub const DEFAULT_PERIOD: u64 = 30;

/// 生成随机 Base32 编码密钥
pub fn generate_base32_secret(secret_bytes: usize) -> String {
    let mut bytes = vec![0u8; secret_bytes.max(DEFAULT_SECRET_BYTES)];
    rand::fill(bytes.as_mut_slice());
    base32_encode(&bytes)
}

/// 构造 otpauth URL
pub fn build_otpauth_url(
    issuer: &str,
    account_name: &str,
    base32_secret: &str,
    digits: u32,
    period: u64,
    algo: Algo,
) -> Result<String, String> {
    validate_digits(digits)?;
    if period == 0 {
        return Err("时间步长必须大于 0".to_string());
    }

    let enc_issuer = urlencoding::encode(issuer);
    let enc_account = urlencoding::encode(account_name);
    let algo_name = match algo {
        Algo::Sha1 => "SHA1",
        Algo::Sha256 => "SHA256",
        Algo::Sha512 => "SHA512",
    };
    let _ = decode_secret(base32_secret)?;

    Ok(format!(
        "otpauth://totp/{enc_issuer}:{enc_account}?secret={base32_secret}&issuer={enc_issuer}&digits={digits}&period={period}&algorithm={algo_name}"
    ))
}

/// 生成当前 TOTP 验证码
pub fn generate_code(
    base32_secret: &str,
    unix_seconds: u64,
    digits: u32,
    period: u64,
    algo: Algo,
) -> Result<String, String> {
    validate_digits(digits)?;
    if period == 0 {
        return Err("时间步长必须大于 0".to_string());
    }

    let key = decode_secret(base32_secret)?;
    let counter = unix_seconds / period;
    compute_hotp(&key, counter, digits, algo)
}

/// 验证用户输入的 TOTP 验证码
pub fn verify_code(
    base32_secret: &str,
    code: &str,
    unix_seconds: u64,
    allowed_drift: i64,
    digits: u32,
    period: u64,
    algo: Algo,
) -> Result<bool, String> {
    validate_digits(digits)?;
    if period == 0 {
        return Err("时间步长必须大于 0".to_string());
    }
    if allowed_drift < 0 {
        return Err("允许漂移窗口不能小于 0".to_string());
    }
    if allowed_drift > 10 {
        return Err("允许漂移窗口不能大于 10".to_string());
    }

    if code.len() != digits as usize || !code.chars().all(|item| item.is_ascii_digit()) {
        return Ok(false);
    }

    let key = decode_secret(base32_secret)?;
    let center = (unix_seconds / period) as i64;

    for drift in -allowed_drift..=allowed_drift {
        let counter = center + drift;
        if counter < 0 {
            continue;
        }

        let candidate = compute_hotp(&key, counter as u64, digits, algo)?;
        if fixed_time_equals(&candidate, code) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// 计算 HOTP
fn compute_hotp(key: &[u8], counter: u64, digits: u32, algo: Algo) -> Result<String, String> {
    let counter_bytes = counter.to_be_bytes();
    let mac = match algo {
        Algo::Sha1 => {
            let mut mac = Hmac::<Sha1>::new_from_slice(key).map_err(|error| error.to_string())?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
        Algo::Sha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|error| error.to_string())?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
        Algo::Sha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(key).map_err(|error| error.to_string())?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
    };

    let offset = (mac[mac.len() - 1] & 0x0F) as usize;
    let binary = ((u32::from(mac[offset]) & 0x7F) << 24)
        | (u32::from(mac[offset + 1]) << 16)
        | (u32::from(mac[offset + 2]) << 8)
        | u32::from(mac[offset + 3]);

    let modulo = 10u64.pow(digits);
    Ok(format!(
        "{:0width$}",
        u64::from(binary) % modulo,
        width = digits as usize
    ))
}

/// 校验位数范围
fn validate_digits(digits: u32) -> Result<(), String> {
    if !(6..=8).contains(&digits) {
        return Err("验证码位数必须在 6 到 8 之间".to_string());
    }

    Ok(())
}

/// 解码并校验 TOTP 密钥
fn decode_secret(base32_secret: &str) -> Result<Vec<u8>, String> {
    let key = base32_decode(base32_secret)?;
    if key.len() < DEFAULT_SECRET_BYTES {
        return Err(format!("密钥长度不能小于 {DEFAULT_SECRET_BYTES} 字节"));
    }

    Ok(key)
}

/// 固定时间比较
fn fixed_time_equals(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0u8;
    for (l, r) in left.bytes().zip(right.bytes()) {
        diff |= l ^ r;
    }

    diff == 0
}

/// Base32 编码
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    if data.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut buffer = u16::from(data[0]);
    let mut next = 1usize;
    let mut bits_left = 8u8;

    while bits_left > 0 || next < data.len() {
        if bits_left < 5 {
            if next < data.len() {
                buffer = (buffer << 8) | u16::from(data[next]);
                next += 1;
                bits_left += 8;
            } else {
                let pad = 5 - bits_left;
                buffer <<= pad;
                bits_left += pad;
            }
        }

        let index = ((buffer >> (bits_left - 5)) & 0x1F) as usize;
        bits_left -= 5;
        result.push(ALPHABET[index] as char);
    }

    result
}

/// Base32 解码
fn base32_decode(base32: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    let mut buffer = 0u32;
    let mut bits_left = 0u8;
    let mut output = Vec::new();

    for raw in base32.to_ascii_uppercase().chars() {
        if raw == '=' || raw.is_whitespace() {
            continue;
        }

        let value = ALPHABET
            .find(raw)
            .ok_or_else(|| format!("无效的 Base32 字符: {raw}"))? as u32;
        buffer = (buffer << 5) | value;
        bits_left += 5;

        if bits_left >= 8 {
            bits_left -= 8;
            output.push(((buffer >> bits_left) & 0xFF) as u8);
        }
    }

    Ok(output)
}
