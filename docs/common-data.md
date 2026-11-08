# 公共数据工具

`common::property_helper` 按模型声明读取字段，`common::data_helper` 提供表格导入与 Excel 导出。需要属性读取或默认 Excel 导出的模型使用 `PropertyModel` 派生；默认导出还要求模型实现 `Default`，用默认实例生成表头，因此空列表也有列标题。

```rust
use common::property_helper::PropertyModel;
use serde::Serialize;

#[derive(Default, Serialize, PropertyModel)]
pub struct ExportRow {
    #[property(name = "Id", display_name = "编号")]
    pub id: i64,
    #[property(name = "Name", display_name = "名称")]
    pub name: String,
    #[property(name = "Enabled", display_name = "启用")]
    pub enabled: bool,
}
```

属性名默认使用 Rust 字段名，`property(name = "...")` 可指定与外部模型对应的名称；`display_name` 用作显示标题。它们独立于 Serde 的 JSON 名称、跳过规则和自定义字段序列化。派生按声明顺序读取 `pub` 字段，普通字段须实现 `Serialize`，标记 `property(enumeration)` 的枚举字段通过 `EnumProperty` 读取。私有字段不参与；显式 `property(skip)` 可排除公共字段，且不要求被排除字段可序列化。

`get_properties(&model)` 返回保留顺序的 `IndexMap<String, DataCell>` 属性字典；`get_properties_display_name(&model)` 改用显示名称，没有显示名称时使用属性名，重复名称返回错误。保留 decimal、可空日期、带时区日期及枚举类型，非可空 `NaiveDateTime` 按源规则先转换为毫秒精度文本。普通集合和嵌套对象不进入这两个标量字典。自定义值对象可用 `property(value_type)` 明确纳入，值采用该类型自身的 Serde 表示，不从字符串猜测类型；枚举使用下述专用标记。

`comparison_entity(&before, &after)` 按声明顺序列出差异，布尔字段显示“是/否”，`NaiveDateTime` 字段只显示日期。普通字符串保留完整内容，即使字符串形状像日期也不会转换。decimal 比较按数值，带时区日期按时间点比较；一般对象仍按序列化快照比较，不提供 .NET 对象引用身份比较。

## Excel 导出

```rust
let rows: Vec<ExportRow> = Vec::new();
let bytes = common::data_helper::list_to_excel_display_name(&rows)?;
```

- `list_to_excel` 使用属性名，`list_to_excel_display_name` 使用显示名称；两者按相同的字段顺序写入数据。
- 输出为 XLSX 字节，可交给现有文件保存或 HTTP 下载逻辑。空列表仍输出默认模型的列标题。
- i64/u64 超出 15 位整数边界时写入文本；decimal 根据去掉前后零后的有效数字数决定数值或文本，避免高精度金额被截断。
- 非可空 `NaiveDateTime` 输出 `yyyy/MM/dd HH:mm:ss.fff`，可空类型有值时输出到秒。`DateTime<Utc/Local/FixedOffset/Tz>` 转换到 `Asia/Shanghai` 后输出到秒；普通日期字符串原样输出。
- `list_to_excel_with_template` 使用属性元数据，模板字段填写 `property(name)` 名称，或未重命名的 Rust 字段名；路径逐段忽略大小写，独立于 JSON 字段名。decimal 和带时区日期沿用上述输出规则，模板内的 `NaiveDateTime` 无论是否可空都输出到秒。

嵌套模型字段声明 `property(nested)`，内部类型也实现 `PropertyModel`。例如：

```rust
#[derive(Serialize, PropertyModel)]
pub struct Address {
    #[property(name = "City")]
    pub city: String,
}

#[derive(Serialize, PropertyModel)]
pub struct ExportWithAddress {
    #[property(name = "Address", nested)]
    pub address: Option<Address>,
}
```

模板 `Field = "Address.City"` 读取城市；`Address` 为 None 时输出空单元格，不继续查找后续路径。非空对象中的缺失字段返回包含完整模板路径的错误。嵌套路径支持 `Option`、引用、`Box`、`Arc` 与 `Rc` 包装；计算属性可手动实现 `PropertyModel::read_property`。未标记为 nested 的字段不能继续按对象路径展开。模板应选择标量叶子；直接导出一般对象或集合仍采用 JSON 文本，不模拟任意 .NET 类型的 `ToString`。

## Excel 导入

`excel_to_data_table(path, has_header)` 读取 `.xls` 或 `.xlsx` 的第一个工作表。第 0 行决定有效列范围；有表头时跳过空白标题，无表头时使用 `column1` 等名称。只在选中列有内容时保留数据行，空白与错误单元格使用空字符串。`DataTableRow` 是保留列顺序的 `IndexMap<String, DataCell>`；`DataCell` 区分整数、浮点数、decimal、日期、带时区日期、文本、空值和 JSON 对象，不把日期单元格提前转成字符串。

导入模型实现 `Default` 并使用 `DataModel` 派生，可与 `PropertyModel` 共用属性声明：

```rust
use common::data_helper::{DataCell, DataModel, DataTableRow, map_to_model_display_name};

#[derive(Default, DataModel)]
pub struct ImportRow {
    #[property(name = "Id", display_name = "编号")]
    pub id: i64,
    #[property(name = "Name", display_name = "名称")]
    pub name: Option<String>,
    #[property(name = "Enabled", display_name = "启用")]
    pub enabled: bool,
}

let mut row = DataTableRow::new();
row.insert("编号".to_owned(), DataCell::Text("123".to_owned()));
row.insert("名称".to_owned(), DataCell::Text(" 示例 ".to_owned()));
let model: ImportRow = map_to_model_display_name(&row)?;
```

`map_to_model` / `data_table_to_list` 只匹配属性名；带 `_display_name` 后缀的入口在属性名未命中时匹配显示名称。名称匹配区分大小写，多个字段使用同一显示名称时选择声明靠前的字段。每行创建一个默认模型并依次处理列；多列指向同一属性时，后面的有效赋值覆盖前面结果。

缺失列、未知列、空值和 `property(read_only)` 字段保留默认值。空白文本清除 `Option<T>`，普通值类型保留默认值。需要允许空白名称清成 null 时使用 `Option<String>`；非可空 `String` 无法表达 null，会返回错误。非空字符串去掉两端空白，错误转换会中止导入并标明列名。

内置 `FromDataCell` 支持 bool、8/16/32/64 位整数、f32/f64、decimal、Guid、char、`NaiveDate/NaiveTime/NaiveDateTime` 和 `DateTime<FixedOffset/Utc/Local>` 及对应 `Option<T>`。数值单元格转整数按中点取偶舍入并检查溢出；整数文本须为整数字符串。bool 接受数值零/非零或 true/false 文本，不把文本 `"1"` 当成 true。字段不受 Serde 的 JSON 重命名或自定义反序列化规则影响；导入专用模型无需实现 `Serialize`。

当前数值文本使用点号小数与逗号分组；日期文本支持 ISO 和常用年月日、月日年格式，没有完整模拟运行环境的区域文化。无偏移日期转 DateTimeOffset 时使用宿主本地时区。历史时区边界、完整 Excel 表头显示格式仍未覆盖。自定义类型可实现 `FromDataCell`，返回 `None` 表示保留当前值，返回 `Some(value)` 表示赋值。

`model_to_map` / `list_to_data_table` 使用 `PropertyModel` 的原始属性名和声明顺序，保留日期与 decimal 类型，也包含公开的集合字段；空列表仍返回 None。`list_to_data_table` 跳过 `Option<T>::None` 对象，非空列表全部为 None 时返回 Some 的空表；真正没有公开字段的非空对象仍生成空行。对象是否为空通过 `PropertyModel::is_null_model` 判断，不能用属性数量代替。集合与一般对象保留为 JSON 单元格，不能据此获得 .NET 对象引用身份或任意对象的自动赋值转换。

## 数据表结构

`DataTable` 独立保存表名、列结构和数据行，通过 `name()`、`columns()`、`rows()` 读取，`len()` / `is_empty()` 只反映数据行。列名及顺序在没有数据行时仍存在。`DataColumn` 包含名称、`DataColumnType` 和 `allow_null`；列默认允许空值，与源 DataColumn 一致，不由 Rust 字段是否为 Option 决定。

Excel 产生没有表名的表，选中的列均声明为 Object，允许同列保留不同单元格类型；只有表头时保留列，无工作表时仍返回 None，没有第 0 行或第 0 行没有有效列时返回零列空表。

模型列表转表使用 `PropertyModel::model_name()`，派生默认采用结构体名称。模型上可声明 `#[property(model_name = "源类型名称")]`，与字段的 `property(name)` 区分；Option、引用、Box、Arc、Rc 委托内部类型名称。现有 LLM 消息及增量模型显式使用源 DTO 名称。首个非空对象建立列，即使首行字段是 None，也按字段声明保留整数宽度、正负号、浮点宽度、日期、decimal、Guid 或枚举类型。所有对象都为 None 时保留表名，但不预先生成列；空输入列表返回 None。

手动组表使用 `DataTable::new(name, columns)?`，再通过 `push_row(row)?` 添加。输入行按表列顺序保存，缺失字段补 Null，未知列及违反 `allow_null` 的值返回错误，整行校验通过后才加入表。列名必须显式提供且不能完全重名；Excel 入口还单独执行其不区分大小写的重复表头检查。

当前类型信息用于保存声明，`push_row` 尚不模拟 .NET DataColumn 的全部赋值转换或类型约束；自定义类型仍保存 Rust 类型名和 JSON 单元格，未实现 CLR 对象身份、默认列名分配、区域名称查找、表达式列或关系约束。不要据此将任意类型的手工赋值视为已经与 DataColumn 等价。

## Guid 转换与短编码

Guid 导入支持 N（32 位十六进制）、D（连字符）、B（大括号）、P（圆括号）及 X（0x 分量与内嵌八字节数组）格式。只去掉首尾空白，X 格式另外允许内部 Unicode 空白；不接受 UUID 库扩展的 `urn:uuid:` 前缀。D/B/P 沿用源 Guid.Parse 的固定宽度分量兼容规则，X 保留前缀、前导零、UInt32 溢出检查以及两个 short 分量取低 16 位的行为，末尾八个分量必须各自落在字节范围。

`common::string_helper::compress_guid`（以及 `compress_uuid` 别名）使用 .NET Guid.ToByteArray 的前三字段小端顺序，生成替换 `+`、`/` 并去掉填充的 22 字符字符串。解码使用相同字节顺序，不把 UUID 标准网络字节序当成 Guid 字节数组。

`decompress_guid` 返回 `Result<Uuid, String>`：无效长度或 Base64 格式对应 `Ok(Uuid::nil())`；Base64 合法但解码后不足 16 字节时返回 Err，对应源代码未捕获的 Guid 字节数组长度异常。长度按去掉首尾空白后的 UTF-16 单元数判断，Base64 内部仅忽略空格、制表符、回车、换行，并接受未使用填充位非零。`decompress_uuid` 保留 Option 入口，将无效输入或长度错误统一返回 None。

## 枚举导入

枚举使用 `DataEnum` 派生并显式声明 `repr(i8/u8/i16/u16/i32/u32/i64/u64)`，成员必须不携带字段；还须实现 `Copy + Debug + Eq`。成员名称默认使用 Rust 名称，可用 `data_enum(name = "...")` 改为源名称，`data_enum(description = "...")` 对应源 `DescriptionAttribute`，两者均独立于 Serde 属性。重复描述允许存在，取声明顺序中的第一个。

导入字段使用 `EnumValue<E>` 或 `Option<EnumValue<E>>`，不能直接将未知整数强制转换为 Rust 枚举。现有 `common::totp_helper::Algo` 和 `application_model::llm::invocation::AppLlmRole` 已提供元数据。例如，`EnumValue<Algo>` 可以接收 `Sha256`、`1` 或未声明的 `99`；`value()` 保留原始整数，`known()` 仅在值准确对应已声明成员时返回该成员。默认值为底层整数零，即使没有名为零的成员也能保留。

转换顺序与源数据工具一致：先按 Int32 解析并以底层位宽转换，再按实际底层整数范围解析，然后匹配名称或逗号分隔的名称组合，最后精确匹配整段描述。名称和描述区分大小写；名称组合按位或，不要求 Flags 标记，不能在组合中混用数字或描述。Int32 分支保留源 `Enum.ToObject` 的截断与符号扩展，例如 u8 枚举输入 `256` 得到零，输入 `-1` 得到 255；超过 Int32 的数字必须符合实际底层范围。空值、空白和可空字段继续使用数据导入的默认值规则。

`EnumValue` 的 JSON 表示是底层数值，不使用名称或描述，也不改变现有业务枚举的 JSON 契约。数字文本仍受上述固定区域规则限制。

## 枚举比较与导出

模型字段声明 `#[property(enumeration)]`，适用于派生 DataEnum 的原生枚举、`EnumValue<E>` 及其 `Option`/引用。该标记不能与 `nested` 或 `value_type` 组合。原生枚举不需要为了属性导出而实现 Serialize；枚举快照始终从底层值与 DataEnum 元数据创建。现有 `AppLlmMessage` 和 `AppLlmStreamDelta` 已声明 Role、Content 的属性信息，JSON 字段名仍沿用原契约。

普通枚举显示准确匹配的成员名称；未声明值输出原始十进制整数。需要 Flags 输出时，在枚举类型上添加 `#[data_enum(flags)]`：先匹配完整值，否则按无符号位模式从大到小选取成员、按从小到大顺序以 `, ` 连接。组合无法覆盖全部位时输出完整整数，不输出部分名称。零优先使用已声明零成员，否则显示 `0`；有符号负值按原位宽分解，无法分解时仍显示带负号整数。描述仅用于导入，不代替输出名称。重复数值别名不能用 Rust 原生枚举派生，手写元数据的同值名称选择不保证与 .NET 的未指定别名顺序一致。

属性比较按枚举类型和底层数值判断变化，差异文本显示上述名称或整数。默认 Excel 导出和模板导出都写入文本单元格，未知数值不会被当成 Excel 数字；可空枚举为空时输出空单元格。

`DataCell::Enum` 保存枚举快照，属性字典和 `model_to_map` 都保留它。重新导入枚举或字符串字段使用名称文本；转整数、浮点、decimal、bool 或 char 时使用原始数值并遵守目标类型的范围规则。`EnumSnapshot::value()` 与 `text()` 分别读取数值和显示文本，不依赖 JSON 序列化形状猜测枚举。

## WebAPI 上传导入

HTTP 处理器取得 `axum::extract::multipart::Field` 后，可调用共享宿主入口：

```rust
let rows = webapi_core::uploaded_file::excel_to_list::<ImportRow>(field).await?;
```

模型实现 `DataModel + Default + Send + 'static`。入口只接受非空 `.xlsx` 文件，扩展名不区分大小写；按上传块写入可执行文件旁 `files/<UUID>.xlsx`，再通过公共数据工具读取首张表并按属性名、显示名称赋值。工作表没有数据时返回空列表。

格式错误或空文件返回“文件格式不正确或文件内容为空”，工作簿或字段转换失败返回“文件内容解析失败”；上传读取、目录及文件写入错误进入宿主内部错误响应。成功、错误和请求中断都由临时文件句柄负责清理。解析在阻塞任务中执行，若请求在解析开始后中断，清理在该任务收尾时完成。
