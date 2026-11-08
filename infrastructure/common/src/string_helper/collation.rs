//! Linux 宿主的默认文化排序，与 Ordinal/OrdinalIgnoreCase 键匹配分开。

use icu_collator::{Collator, CollatorBorrowed, options::CollatorOptions};
use icu_locale_core::Locale;
use std::sync::LazyLock;

static COLLATOR: LazyLock<Result<CollatorBorrowed<'static>, String>> = LazyLock::new(|| {
    let locale = system_locale()?;
    Collator::try_new(locale.into(), CollatorOptions::default())
        .map_err(|error| format!("文化排序初始化失败: {error}"))
});

/// 按宿主文化稳定排序；等价文本保留输入顺序，不追加 Ordinal 次级比较。
pub fn sort_current_culture_by<T>(
    values: &mut [T],
    key: impl Fn(&T) -> &str,
) -> Result<(), String> {
    if values.len() < 2 {
        return Ok(());
    }
    let collator = COLLATOR.as_ref().map_err(Clone::clone)?;
    values.sort_by(|left, right| collator.compare(key(left), key(right)));
    Ok(())
}

fn system_locale() -> Result<Locale, String> {
    // ICU 在默认 C locale 下依次读取这些环境变量；空变量同样覆盖下一来源。
    let value = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(std::env::var_os)
        .unwrap_or_default()
        .into_string()
        .map_err(|_| "宿主文化名称不是有效的 UTF-8".to_owned())?;
    if value.contains('@') {
        return Err("宿主文化排序暂不支持 POSIX @ 修饰符，请使用 BCP 47 文化名称".to_owned());
    }
    let name = value
        .split('.')
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    // .NET 在 Linux 上将 ICU 的 en_US_POSIX（含 C/C.UTF-8）映射到 invariant。
    if name.is_empty() || matches!(name.as_str(), "C" | "POSIX" | "en-US-POSIX") {
        return Ok(Locale::UNKNOWN);
    }
    name.parse().map_err(|_| "宿主文化名称无效".to_owned())
}
