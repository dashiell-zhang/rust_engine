use aes::{Aes128, Aes192, Aes256};
use aes_gcm::{
    Aes128Gcm, Aes256Gcm, AesGcm, KeyInit as AesGcmKeyInit, Nonce,
    aead::{Aead, Payload},
    aes::Aes192 as Aes192GcmCipher,
};
use base64::{Engine as _, engine::general_purpose};
use ecb::cipher::{
    BlockModeDecrypt, BlockModeEncrypt, KeyInit as EcbKeyInit, block_padding::Pkcs7,
};
use hmac::{Hmac, Mac};
use md5::Md5;
use pbkdf2::pbkdf2;
use rsa::{
    Oaep, Pkcs1v15Encrypt, Pkcs1v15Sign, Pss, RsaPrivateKey, RsaPublicKey,
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};
use subtle::ConstantTimeEq;

type Aes192Gcm = AesGcm<Aes192GcmCipher, aes_gcm::aead::consts::U12>;
type Aes128EcbEnc = ecb::Encryptor<Aes128>;
type Aes192EcbEnc = ecb::Encryptor<Aes192>;
type Aes256EcbEnc = ecb::Encryptor<Aes256>;
type Aes128EcbDec = ecb::Decryptor<Aes128>;
type Aes192EcbDec = ecb::Decryptor<Aes192>;
type Aes256EcbDec = ecb::Decryptor<Aes256>;

/// 字符串编码格式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringEncoding {
    /// Base64 编码
    Base64,

    /// 十六进制编码
    Hex,
}

impl StringEncoding {
    /// 解析编码格式文本
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "base64" => Some(Self::Base64),
            "hex" => Some(Self::Hex),
            _ => None,
        }
    }
}

/// RSA 摘要算法
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaHashAlgorithm {
    /// MD5
    Md5,

    /// SHA1
    Sha1,

    /// SHA256
    Sha256,

    /// SHA512
    Sha512,
}

/// RSA 签名填充模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaSignaturePadding {
    /// PSS
    Pss,

    /// PKCS1 v1.5
    Pkcs1,
}

/// RSA 加密填充模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaEncryptionPadding {
    /// PKCS1 v1.5
    Pkcs1,

    /// OAEP SHA1
    OaepSha1,

    /// OAEP SHA256
    OaepSha256,

    /// OAEP SHA512
    OaepSha512,
}

/// Base64 编码
pub fn base64_encode(text: &str) -> String {
    general_purpose::STANDARD.encode(text.as_bytes())
}

/// Base64 解码
pub fn base64_decode(text: &str) -> Result<String, String> {
    let bytes = general_purpose::STANDARD
        .decode(text)
        .map_err(|error| error.to_string())?;

    String::from_utf8(bytes).map_err(|error| error.to_string())
}

/// 使用 Aes ECB 加密
pub fn aes_encrypt(
    text: &str,
    private_key: &str,
    string_encoding: StringEncoding,
) -> Result<String, String> {
    let encrypted = aes_ecb_encrypt_bytes(text.as_bytes(), private_key.as_bytes())?;
    Ok(encode_bytes(&encrypted, string_encoding))
}

/// 使用 Aes ECB 解密
pub fn aes_decrypt(
    cipher_text: &str,
    private_key: &str,
    string_encoding: StringEncoding,
) -> Result<String, String> {
    let encrypted = decode_bytes(cipher_text, string_encoding)?;
    let decrypted = aes_ecb_decrypt_bytes(&encrypted, private_key.as_bytes())?;
    String::from_utf8(decrypted).map_err(|error| error.to_string())
}

/// 使用 AesGcm 加密
pub fn aes_gcm_encrypt(
    text: &str,
    private_key: &str,
    nonce: &str,
    associated_text: Option<&str>,
    string_encoding: StringEncoding,
) -> Result<String, String> {
    let encrypted = aes_gcm_encrypt_bytes(
        text.as_bytes(),
        private_key.as_bytes(),
        nonce.as_bytes(),
        associated_text.map(str::as_bytes),
    )?;
    Ok(encode_bytes(&encrypted, string_encoding))
}

/// 使用 AesGcm 解密
pub fn aes_gcm_decrypt(
    cipher_text: &str,
    private_key: &str,
    nonce: &str,
    associated_text: Option<&str>,
    string_encoding: StringEncoding,
) -> Result<String, String> {
    let encrypted = decode_bytes(cipher_text, string_encoding)?;
    let decrypted = aes_gcm_decrypt_bytes(
        &encrypted,
        private_key.as_bytes(),
        nonce.as_bytes(),
        associated_text.map(str::as_bytes),
    )?;
    String::from_utf8(decrypted).map_err(|error| error.to_string())
}

/// MD5 摘要计算
pub fn md5_hash_data(text: &str, encoding: StringEncoding) -> String {
    encode_bytes(&Md5::digest(text.as_bytes()), encoding)
}

/// SHA1 摘要计算
pub fn sha1_hash_data(text: &str, encoding: StringEncoding) -> String {
    encode_bytes(&Sha1::digest(text.as_bytes()), encoding)
}

/// SHA256 摘要计算
pub fn sha256_hash_data(text: &str, encoding: StringEncoding) -> String {
    encode_bytes(&Sha256::digest(text.as_bytes()), encoding)
}

/// SHA384 摘要计算
pub fn sha384_hash_data(text: &str, encoding: StringEncoding) -> String {
    encode_bytes(&Sha384::digest(text.as_bytes()), encoding)
}

/// SHA512 摘要计算
pub fn sha512_hash_data(text: &str, encoding: StringEncoding) -> String {
    encode_bytes(&Sha512::digest(text.as_bytes()), encoding)
}

/// HMAC MD5 摘要计算
pub fn hmac_md5_hash_data(text: &str, private_key: &str, encoding: StringEncoding) -> String {
    type HmacMd5 = Hmac<Md5>;

    let mut mac = <HmacMd5 as Mac>::new_from_slice(private_key.as_bytes())
        .expect("HMAC-MD5 密钥长度初始化失败");
    mac.update(text.as_bytes());
    encode_bytes(&mac.finalize().into_bytes(), encoding)
}

/// HMAC SHA1 摘要计算
pub fn hmac_sha1_hash_data(text: &str, private_key: &str, encoding: StringEncoding) -> String {
    type HmacSha1 = Hmac<Sha1>;

    let mut mac = <HmacSha1 as Mac>::new_from_slice(private_key.as_bytes())
        .expect("HMAC-SHA1 密钥长度初始化失败");
    mac.update(text.as_bytes());
    encode_bytes(&mac.finalize().into_bytes(), encoding)
}

/// HMAC SHA256 摘要计算
pub fn hmac_sha256_hash_data(text: &str, private_key: &str, encoding: StringEncoding) -> String {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = <HmacSha256 as Mac>::new_from_slice(private_key.as_bytes())
        .expect("HMAC-SHA256 密钥长度初始化失败");
    mac.update(text.as_bytes());
    encode_bytes(&mac.finalize().into_bytes(), encoding)
}

/// HMAC SHA384 摘要计算
pub fn hmac_sha384_hash_data(text: &str, private_key: &str, encoding: StringEncoding) -> String {
    type HmacSha384 = Hmac<Sha384>;

    let mut mac = <HmacSha384 as Mac>::new_from_slice(private_key.as_bytes())
        .expect("HMAC-SHA384 密钥长度初始化失败");
    mac.update(text.as_bytes());
    encode_bytes(&mac.finalize().into_bytes(), encoding)
}

/// HMAC SHA512 摘要计算
pub fn hmac_sha512_hash_data(text: &str, private_key: &str, encoding: StringEncoding) -> String {
    type HmacSha512 = Hmac<Sha512>;

    let mut mac = <HmacSha512 as Mac>::new_from_slice(private_key.as_bytes())
        .expect("HMAC-SHA512 密钥长度初始化失败");
    mac.update(text.as_bytes());
    encode_bytes(&mac.finalize().into_bytes(), encoding)
}

/// RSA 签名计算
pub fn rsa_sign_data(
    private_key: &str,
    content: &str,
    string_encoding: StringEncoding,
    hash_algorithm: RsaHashAlgorithm,
    signature_padding: RsaSignaturePadding,
) -> Result<String, String> {
    let private_key = parse_rsa_private_key(private_key)?;
    let mut rng = rsa::rand_core::OsRng;
    let digest = digest_for_rsa(hash_algorithm, content.as_bytes());

    let signature = match signature_padding {
        RsaSignaturePadding::Pkcs1 => match hash_algorithm {
            RsaHashAlgorithm::Md5 => private_key
                .sign(pkcs1v15_sign_padding(hash_algorithm), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha1 => private_key
                .sign(pkcs1v15_sign_padding(hash_algorithm), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha256 => private_key
                .sign(pkcs1v15_sign_padding(hash_algorithm), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha512 => private_key
                .sign(pkcs1v15_sign_padding(hash_algorithm), &digest)
                .map_err(|error| error.to_string())?,
        },
        RsaSignaturePadding::Pss => match hash_algorithm {
            RsaHashAlgorithm::Md5 => private_key
                .sign_with_rng(&mut rng, Pss::new::<Md5>(), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha1 => private_key
                .sign_with_rng(&mut rng, Pss::new::<Sha1>(), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha256 => private_key
                .sign_with_rng(&mut rng, Pss::new::<Sha256>(), &digest)
                .map_err(|error| error.to_string())?,
            RsaHashAlgorithm::Sha512 => private_key
                .sign_with_rng(&mut rng, Pss::new::<Sha512>(), &digest)
                .map_err(|error| error.to_string())?,
        },
    };

    Ok(encode_bytes(&signature, string_encoding))
}

/// RSA 签名验证
pub fn rsa_verify_data(
    public_key: &str,
    content: &str,
    signature: &str,
    string_encoding: StringEncoding,
    hash_algorithm: RsaHashAlgorithm,
    signature_padding: RsaSignaturePadding,
) -> Result<bool, String> {
    let public_key = parse_rsa_public_key(public_key)?;
    let signature = decode_bytes(signature, string_encoding)?;
    let digest = digest_for_rsa(hash_algorithm, content.as_bytes());

    let result = match signature_padding {
        RsaSignaturePadding::Pkcs1 => match hash_algorithm {
            RsaHashAlgorithm::Md5 => {
                public_key.verify(pkcs1v15_sign_padding(hash_algorithm), &digest, &signature)
            }
            RsaHashAlgorithm::Sha1 => {
                public_key.verify(pkcs1v15_sign_padding(hash_algorithm), &digest, &signature)
            }
            RsaHashAlgorithm::Sha256 => {
                public_key.verify(pkcs1v15_sign_padding(hash_algorithm), &digest, &signature)
            }
            RsaHashAlgorithm::Sha512 => {
                public_key.verify(pkcs1v15_sign_padding(hash_algorithm), &digest, &signature)
            }
        },
        RsaSignaturePadding::Pss => match hash_algorithm {
            RsaHashAlgorithm::Md5 => public_key.verify(Pss::new::<Md5>(), &digest, &signature),
            RsaHashAlgorithm::Sha1 => public_key.verify(Pss::new::<Sha1>(), &digest, &signature),
            RsaHashAlgorithm::Sha256 => {
                public_key.verify(Pss::new::<Sha256>(), &digest, &signature)
            }
            RsaHashAlgorithm::Sha512 => {
                public_key.verify(Pss::new::<Sha512>(), &digest, &signature)
            }
        },
    };

    Ok(result.is_ok())
}

/// RSA 加密
pub fn rsa_encrypt(
    public_key: &str,
    content: &str,
    string_encoding: StringEncoding,
    encryption_padding: RsaEncryptionPadding,
) -> Result<String, String> {
    let public_key = parse_rsa_public_key(public_key)?;
    let mut rng = rsa::rand_core::OsRng;
    let encrypted = match encryption_padding {
        RsaEncryptionPadding::Pkcs1 => public_key
            .encrypt(&mut rng, Pkcs1v15Encrypt, content.as_bytes())
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha1 => public_key
            .encrypt(&mut rng, Oaep::new::<Sha1>(), content.as_bytes())
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha256 => public_key
            .encrypt(&mut rng, Oaep::new::<Sha256>(), content.as_bytes())
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha512 => public_key
            .encrypt(&mut rng, Oaep::new::<Sha512>(), content.as_bytes())
            .map_err(|error| error.to_string())?,
    };

    Ok(encode_bytes(&encrypted, string_encoding))
}

/// RSA 解密
pub fn rsa_decrypt(
    private_key: &str,
    content: &str,
    string_encoding: StringEncoding,
    encryption_padding: RsaEncryptionPadding,
) -> Result<String, String> {
    let private_key = parse_rsa_private_key(private_key)?;
    let content = decode_bytes(content, string_encoding)?;
    let decrypted = match encryption_padding {
        RsaEncryptionPadding::Pkcs1 => private_key
            .decrypt(Pkcs1v15Encrypt, &content)
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha1 => private_key
            .decrypt(Oaep::new::<Sha1>(), &content)
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha256 => private_key
            .decrypt(Oaep::new::<Sha256>(), &content)
            .map_err(|error| error.to_string())?,
        RsaEncryptionPadding::OaepSha512 => private_key
            .decrypt(Oaep::new::<Sha512>(), &content)
            .map_err(|error| error.to_string())?,
    };

    String::from_utf8(decrypted).map_err(|error| error.to_string())
}

/// 使用 PBKDF2-SHA256 计算密码哈希
pub fn hash_password_pbkdf2_sha256(password: &str, salt: &str) -> String {
    let mut derived_key = [0u8; 32];
    let _ = pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt.as_bytes(), 1000, &mut derived_key);

    general_purpose::STANDARD.encode(derived_key)
}

/// 使用 PBKDF2-SHA256 验证密码哈希
pub fn verify_password_pbkdf2_sha256(input_password: &str, salt: &str, stored_hash: &str) -> bool {
    let computed_hash = hash_password_pbkdf2_sha256(input_password, salt);

    computed_hash
        .as_bytes()
        .ct_eq(stored_hash.as_bytes())
        .into()
}

/// 执行 AES ECB 加密
fn aes_ecb_encrypt_bytes(plain: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    match key.len() {
        16 => encrypt_ecb_with::<Aes128EcbEnc>(plain, key),
        24 => encrypt_ecb_with::<Aes192EcbEnc>(plain, key),
        32 => encrypt_ecb_with::<Aes256EcbEnc>(plain, key),
        _ => Err("private_key 长度必须是 16、24 或 32 字节".to_string()),
    }
}

/// 执行 AES ECB 解密
fn aes_ecb_decrypt_bytes(cipher: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    match key.len() {
        16 => decrypt_ecb_with::<Aes128EcbDec>(cipher, key),
        24 => decrypt_ecb_with::<Aes192EcbDec>(cipher, key),
        32 => decrypt_ecb_with::<Aes256EcbDec>(cipher, key),
        _ => Err("private_key 长度必须是 16、24 或 32 字节".to_string()),
    }
}

/// 使用指定 ECB 算法加密
fn encrypt_ecb_with<C>(plain: &[u8], key: &[u8]) -> Result<Vec<u8>, String>
where
    C: EcbKeyInit + BlockModeEncrypt,
{
    let mut buffer = plain.to_vec();
    let msg_len = buffer.len();
    let block_size = 16;
    let padding = block_size - (msg_len % block_size);
    buffer.resize(msg_len + padding, 0);

    let encrypted = C::new_from_slice(key)
        .map_err(|error| error.to_string())?
        .encrypt_padded::<Pkcs7>(&mut buffer, msg_len)
        .map_err(|error| error.to_string())?;

    Ok(encrypted.to_vec())
}

/// 使用指定 ECB 算法解密
fn decrypt_ecb_with<C>(cipher: &[u8], key: &[u8]) -> Result<Vec<u8>, String>
where
    C: EcbKeyInit + BlockModeDecrypt,
{
    let mut buffer = cipher.to_vec();
    let decrypted = C::new_from_slice(key)
        .map_err(|error| error.to_string())?
        .decrypt_padded::<Pkcs7>(&mut buffer)
        .map_err(|error| error.to_string())?;

    Ok(decrypted.to_vec())
}

/// 执行 AES GCM 加密
fn aes_gcm_encrypt_bytes(
    plain: &[u8],
    key: &[u8],
    nonce: &[u8],
    associated_text: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if nonce.len() != 12 {
        return Err("nonce 长度必须是 12 字节".to_string());
    }

    let payload = Payload {
        msg: plain,
        aad: associated_text.unwrap_or_default(),
    };

    match key.len() {
        16 => Aes128Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .encrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        24 => Aes192Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .encrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        32 => Aes256Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .encrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        _ => Err("private_key 长度必须是 16、24 或 32 字节".to_string()),
    }
}

/// 执行 AES GCM 解密
fn aes_gcm_decrypt_bytes(
    cipher: &[u8],
    key: &[u8],
    nonce: &[u8],
    associated_text: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if nonce.len() != 12 {
        return Err("nonce 长度必须是 12 字节".to_string());
    }

    let payload = Payload {
        msg: cipher,
        aad: associated_text.unwrap_or_default(),
    };

    match key.len() {
        16 => Aes128Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .decrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        24 => Aes192Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .decrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        32 => Aes256Gcm::new_from_slice(key)
            .map_err(|error| error.to_string())?
            .decrypt(Nonce::from_slice(nonce), payload)
            .map_err(|error| error.to_string()),
        _ => Err("private_key 长度必须是 16、24 或 32 字节".to_string()),
    }
}

/// 解析 RSA 私钥
fn parse_rsa_private_key(value: &str) -> Result<RsaPrivateKey, String> {
    let normalized = value.trim();

    if normalized.starts_with("-----BEGIN") {
        return RsaPrivateKey::from_pkcs1_pem(normalized)
            .or_else(|_| RsaPrivateKey::from_pkcs8_pem(normalized))
            .map_err(|error| error.to_string());
    }

    let decoded = general_purpose::STANDARD
        .decode(normalized.replace(['\r', '\n'], ""))
        .map_err(|error| error.to_string())?;

    RsaPrivateKey::from_pkcs1_der(&decoded)
        .or_else(|_| RsaPrivateKey::from_pkcs8_der(&decoded))
        .map_err(|error| error.to_string())
}

/// 解析 RSA 公钥
fn parse_rsa_public_key(value: &str) -> Result<RsaPublicKey, String> {
    let normalized = value.trim();

    if normalized.starts_with("-----BEGIN") {
        return RsaPublicKey::from_pkcs1_pem(normalized)
            .or_else(|_| RsaPublicKey::from_public_key_pem(normalized))
            .map_err(|error| error.to_string());
    }

    let decoded = general_purpose::STANDARD
        .decode(normalized.replace(['\r', '\n'], ""))
        .map_err(|error| error.to_string())?;

    RsaPublicKey::from_pkcs1_der(&decoded)
        .or_else(|_| RsaPublicKey::from_public_key_der(&decoded))
        .map_err(|error| error.to_string())
}

/// 按指定摘要算法生成摘要
fn digest_for_rsa(hash_algorithm: RsaHashAlgorithm, content: &[u8]) -> Vec<u8> {
    match hash_algorithm {
        RsaHashAlgorithm::Md5 => Md5::digest(content).to_vec(),
        RsaHashAlgorithm::Sha1 => Sha1::digest(content).to_vec(),
        RsaHashAlgorithm::Sha256 => Sha256::digest(content).to_vec(),
        RsaHashAlgorithm::Sha512 => Sha512::digest(content).to_vec(),
    }
}

/// 构造标准 PKCS#1 v1.5 签名填充
fn pkcs1v15_sign_padding(hash_algorithm: RsaHashAlgorithm) -> Pkcs1v15Sign {
    let (hash_len, prefix): (usize, &[u8]) = match hash_algorithm {
        RsaHashAlgorithm::Md5 => (
            16,
            &[
                0x30, 0x20, 0x30, 0x0C, 0x06, 0x08, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x02, 0x05,
                0x05, 0x00, 0x04, 0x10,
            ],
        ),
        RsaHashAlgorithm::Sha1 => (
            20,
            &[
                0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2B, 0x0E, 0x03, 0x02, 0x1A, 0x05, 0x00, 0x04,
                0x14,
            ],
        ),
        RsaHashAlgorithm::Sha256 => (
            32,
            &[
                0x30, 0x31, 0x30, 0x0D, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x01, 0x05, 0x00, 0x04, 0x20,
            ],
        ),
        RsaHashAlgorithm::Sha512 => (
            64,
            &[
                0x30, 0x51, 0x30, 0x0D, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x03, 0x05, 0x00, 0x04, 0x40,
            ],
        ),
    };

    Pkcs1v15Sign {
        hash_len: Some(hash_len),
        prefix: prefix.into(),
    }
}

/// 解析字节文本
fn decode_bytes(value: &str, encoding: StringEncoding) -> Result<Vec<u8>, String> {
    match encoding {
        StringEncoding::Base64 => general_purpose::STANDARD
            .decode(value)
            .map_err(|error| error.to_string()),
        StringEncoding::Hex => decode_hex(value),
    }
}

/// 按指定格式编码二进制内容
fn encode_bytes(bytes: &[u8], encoding: StringEncoding) -> String {
    match encoding {
        StringEncoding::Base64 => general_purpose::STANDARD.encode(bytes),
        StringEncoding::Hex => hex_upper(bytes),
    }
}

/// 解析十六进制字符串
fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) {
        return Err("十六进制字符串长度必须是偶数".to_string());
    }

    let mut output = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();

    for index in (0..bytes.len()).step_by(2) {
        let high = hex_value(bytes[index])?;
        let low = hex_value(bytes[index + 1])?;
        output.push((high << 4) | low);
    }

    Ok(output)
}

/// 单个十六进制字符转数值
fn hex_value(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("十六进制字符串包含无效字符".to_string()),
    }
}

/// 输出大写十六进制字符串
fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0F) as usize] as char);
    }

    output
}
