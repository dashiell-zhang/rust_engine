# 公共字符串工具

`common::string_helper` 提供模板替换、URL 参数生成、文本清理、截断、压缩和地址判断，可用于通知模板、导出链接与内容摘要。

## 模板替换

```rust
use std::collections::HashMap;
use common::string_helper::replace_placeholders;

let values = HashMap::from([("name".to_owned(), "小明".to_owned())]);
let text = replace_placeholders("你好，{{name}}", &values, "{{", "}}")?;
```

结果为 `你好，小明`。替换值原样插入；缺失参数立即返回错误，不把未赋值占位符留在结果中。前后缀按字面量匹配，参数名不跨行。

## URL 参数

`model_to_uri_param` 根据模型的 `PropertyModel` 属性声明生成参数。名称、顺序和显示元数据见[公共数据工具](common-data.md)。

空值和全空白文本不生成参数；其他文本保留首尾空白。名称和值按 UTF-8 编码，空格写为 `+`；已有查询串时自动选择 `&`。布尔值输出 `True`/`False`，枚举输出底层数字。

不要用 JSON 重命名规则推断 URL 参数名；此工具读取 `property(name)` 或 Rust 字段名。

## 文本清理

| 方法 | 用途 |
| --- | --- |
| `remove_punctuation` | 移除 BMP 范围的 Unicode 标点 |
| `remove_number` / `remove_not_number` | 移除数字 / 只保留数字 |
| `contains_cjk` / `is_all_cjk` | 判断是否包含中文 / 是否全为中文 |
| `remove_emoji` | 仅保留中文、ASCII 字母数字、空格及 `._-` |
| `remove_html` | 解码实体并移除 HTML 标记，适合生成纯文本摘要 |
| `unicode_to_string` | 读取字符转义，非法转义返回错误 |

```rust
let digits = common::string_helper::remove_not_number("订单 A123");
let text = common::string_helper::remove_html("<p>欢迎使用</p>");
```

`remove_emoji` 是字符白名单过滤器，可能移除非 Emoji 的文字。`remove_html` 用于提取文本，不能作为允许任意 HTML 安全展示的净化器。

## 截断与隐藏

```rust
let summary = common::string_helper::substring_text("abcdef", 3)?;
let hidden = common::string_helper::partially_hidden("123456789")?;
```

截断结果为 `abc...`。长度按 UTF-16 单元计算；隐藏方法遮盖中间一组字符。拆开 Unicode 代理对时 String 入口返回错误，需要保留原始 UTF-16 单元时使用对应的 `*_utf16` 入口。

## 压缩与短 ID

`compress_string` 将 UTF-8 文本压缩为 gzip 后编码为 Base64，`decompress_string` 还原文本。解压可能返回截断输入中已读出的部分，因此不能把它当作文件完整性校验。

```rust
let packed = common::string_helper::compress_string("需要保存的文本")?;
let restored = common::string_helper::decompress_string(&packed)?;
```

`compress_guid` 将 UUID 编码为 22 字符短串，`decompress_guid` 还原。短编码采用 Guid 的前三字段小端字节顺序；与 UUID 网络字节序的编码不可互换。详细格式见[Guid 转换与短编码](common-data.md#guid-转换与短编码)。

## 地址判断

```rust
let local = common::string_helper::is_lan_ip_address("192.168.1.10");
let matched = common::string_helper::is_ip_in_cidr_range_v4("10.1.2.3", "10.0.0.0/8");
```

局域网范围包括 IPv4 的 `10/8`、`172.16/12`、`192.168/16` 和 IPv6 的 `fc00::/7`，不包含回环和链路本地地址。地址分类不替代对下载来源、DNS 解析结果和重定向目标的完整校验。
