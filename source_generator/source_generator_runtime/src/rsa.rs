//! 请求模型中 RSA 加密字符串的显式遍历契约，不绑定 HTTP 或密钥存储。
use std::collections::{BTreeMap, HashMap};

/// 处理一个已标记的字符串；错误类型不包含明文或密文。
pub type DecryptField<'a> = dyn FnMut(&mut String) -> Result<(), String> + 'a;

/// 模型递归遍历能力，由同名 derive 为请求 DTO 生成。
pub trait RsaDecrypt {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String>;
}

/// 只允许 String 与 Option<String> 使用 rsa_encrypted 标记。
pub trait EncryptedField {
    fn decrypt_field(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String>;
}
impl EncryptedField for String {
    fn decrypt_field(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        decrypt(self)
    }
}
impl EncryptedField for Option<String> {
    fn decrypt_field(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        if let Some(value) = self {
            decrypt(value)?;
        }
        Ok(())
    }
}

macro_rules! leaf {
    ($($ty:ty),* $(,)?) => { $(impl RsaDecrypt for $ty {
        fn decrypt_fields(&mut self, _: &mut DecryptField<'_>) -> Result<(), String> { Ok(()) }
    })* };
}
leaf!(
    (),
    bool,
    char,
    String,
    str,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    serde_json::Value,
    uuid::Uuid,
    chrono::NaiveDate,
    chrono::NaiveDateTime,
    chrono::NaiveTime,
    chrono::Duration,
    std::time::Duration,
    std::time::SystemTime
);
impl<T: chrono::TimeZone> RsaDecrypt for chrono::DateTime<T> {
    fn decrypt_fields(&mut self, _: &mut DecryptField<'_>) -> Result<(), String> {
        Ok(())
    }
}
impl<T> RsaDecrypt for std::marker::PhantomData<T> {
    fn decrypt_fields(&mut self, _: &mut DecryptField<'_>) -> Result<(), String> {
        Ok(())
    }
}
impl<T: RsaDecrypt> RsaDecrypt for Option<T> {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        if let Some(value) = self {
            value.decrypt_fields(decrypt)?;
        }
        Ok(())
    }
}
impl<T: RsaDecrypt + ?Sized> RsaDecrypt for Box<T> {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        (**self).decrypt_fields(decrypt)
    }
}
impl<T: RsaDecrypt> RsaDecrypt for [T] {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        for value in self {
            value.decrypt_fields(decrypt)?;
        }
        Ok(())
    }
}
impl<T: RsaDecrypt, const N: usize> RsaDecrypt for [T; N] {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        self.as_mut_slice().decrypt_fields(decrypt)
    }
}
impl<T: RsaDecrypt> RsaDecrypt for Vec<T> {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        self.as_mut_slice().decrypt_fields(decrypt)
    }
}
impl<K, V: RsaDecrypt> RsaDecrypt for BTreeMap<K, V> {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        for value in self.values_mut() {
            value.decrypt_fields(decrypt)?;
        }
        Ok(())
    }
}
impl<K, V: RsaDecrypt, S> RsaDecrypt for HashMap<K, V, S> {
    fn decrypt_fields(&mut self, decrypt: &mut DecryptField<'_>) -> Result<(), String> {
        for value in self.values_mut() {
            value.decrypt_fields(decrypt)?;
        }
        Ok(())
    }
}
