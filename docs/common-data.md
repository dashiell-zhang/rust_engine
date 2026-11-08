# 公共数据与 Excel 工具

`common::property_helper` 读取模型属性，`common::data_helper` 提供表格转换、Excel 导入导出和数据转换。适合报表、导入模板和字段差异展示。

## 声明模型

导出模型派生 PropertyModel，使用 property 指定属性名和显示标题：

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

只读取公开字段，顺序与声明一致。`property(skip)` 排除字段；属性名与 Serde 的 JSON 重命名相互独立。

`get_properties` 返回属性名字典，`get_properties_display_name` 使用显示名称。`comparison_entity` 比较两个模型并返回字段差异，适合生成变更说明。

## Excel 导出

```rust
let rows = vec![ExportRow {
    id: 123,
    name: "示例".to_owned(),
    enabled: true,
}];
let bytes = common::data_helper::list_to_excel_display_name(&rows)?;
```

返回 XLSX 字节，可交给文件保存或 HTTP 下载。使用 `list_to_excel` 时表头为属性名；空列表仍根据 Default 模型生成表头。

- 超过十五位的整数及高精度 decimal 可写为文本，避免 Excel 数值精度损失。
- 带时区日期转为 Asia/Shanghai 后输出到秒；普通日期字符串原样保留。
- 非可空 NaiveDateTime 默认输出到毫秒，可空值输出到秒。
- `list_to_excel_with_template` 按模板属性路径输出，使用属性名而非 JSON 名。

## 嵌套模板字段

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

模板字段 `Address.City` 读取城市，路径忽略大小写。Address 为 None 时输出空单元格；非空对象缺少字段时报错。模板应选择标量叶子，一般对象和集合导出为 JSON 文本。

## Excel 导入

`excel_to_data_table(path, has_header)` 读取 xls/xlsx 的第一张工作表。有表头时跳过空标题，无表头时使用 column1 等列名。

导入模型实现 Default 和 DataModel：

```rust
use common::data_helper::{DataCell, DataModel, DataTableRow, map_to_model_display_name};

#[derive(Default, DataModel)]
pub struct ImportRow {
    #[property(name = "Id", display_name = "编号")]
    pub id: i64,
    #[property(name = "Name", display_name = "名称")]
    pub name: Option<String>,
}

let mut row = DataTableRow::new();
row.insert("编号".to_owned(), DataCell::Text("123".to_owned()));
row.insert("名称".to_owned(), DataCell::Text(" 示例 ".to_owned()));
let model: ImportRow = map_to_model_display_name(&row)?;
```

`map_to_model` / `data_table_to_list` 只匹配属性名，带 `_display_name` 的方法还匹配显示名称，匹配区分大小写。

未知列、缺失列和只读字段保留默认值。非空字符串裁剪首尾空白；空白可清除 Option，非可空 String 无法接收 null。转换失败中止导入并标明列名。

内置转换支持常用数值、decimal、bool、UUID、日期和对应可空类型。数值单元格转整数采用中点取偶舍入并检查溢出；文本整数须为整数字符串。bool 接受数值零/非零或 true/false 文本，不把文本 "1" 当作 true。

自定义类型实现 `FromDataCell`。日期文本使用固定解析规则，不随任意区域文化变化；需要精确时间点时提供明确时区偏移。

## 数据表结构

DataTable 独立保存表名、列定义和行，空表仍可保留列。`DataColumn` 声明名称、类型和 allow_null，`DataTableRow` 保留列顺序。

手动组表使用 `DataTable::new(name, columns)?`，再通过 `push_row(row)?` 添加。未知列和违反可空约束的值报错，缺失字段补 Null。列类型用于保存声明，不代表支持任意类型的自动赋值转换。

`model_to_map` 和 `list_to_data_table` 按模型属性转换，保留日期、decimal 和枚举；空输入列表返回 None。模型内的集合和一般对象保留为 JSON 单元格。

## 枚举导入与导出

无字段枚举可派生 DataEnum，显式声明整数 repr，并实现 Copy、Debug、Eq。成员可使用 `data_enum(name = "...", description = "...")` 定义外部名称及描述。

导入字段使用 `EnumValue<E>` 或其 Option，允许保留未声明的整数。`value()` 返回原始值，`known()` 仅在匹配枚举成员时返回 Some。

导出字段标记 `#[property(enumeration)]`。已知值输出成员名，未知值输出十进制文本；描述用于导入，不替代输出名称。`#[data_enum(flags)]` 允许组合名称显示。

## Guid 转换与短编码

Guid 导入支持 N、D、B、P、X 格式，允许首尾空白，不接受 urn:uuid: 前缀。

```rust
let id = uuid::Uuid::new_v4();
let short = common::string_helper::compress_guid(id);
let restored = common::string_helper::decompress_guid(&short)?;
```

短串为 22 字符，采用 Guid 的前三字段小端顺序。`decompress_guid` 对无效长度或 Base64 返回空 UUID，对解码后不足十六字节返回错误；`decompress_uuid` 则将无效输入统一返回 None。

## WebAPI 上传导入

取得 multipart Field 后：

```rust
let rows = webapi_core::uploaded_file::excel_to_list::<ImportRow>(field).await?;
```

模型要求 `DataModel + Default + Send + 'static`。HTTP 入口只接受非空 xlsx，读取首张表并按属性名或显示名称赋值，没有数据时返回空列表。

上传内容暂存在可执行文件旁的 files 目录，该目录须可写；处理结束后自动清理。文件格式或模型转换失败返回明确业务错误，目录和读写故障进入宿主内部错误响应。
