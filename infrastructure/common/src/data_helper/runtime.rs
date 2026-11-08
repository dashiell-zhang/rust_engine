use calamine::{Data, Range, Reader, Sheets, Xls, Xlsx};
use chrono::NaiveDate;
use indexmap::IndexMap;
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize, ser::Error};
use serde_json::Value;
use std::collections::HashSet;
use std::{fs::File, io::BufReader, path::Path};

use crate::property_helper::{
    Property, PropertyKind, PropertyModel, get_properties, get_properties_display_name,
    get_property,
};
use crate::string_helper::ordinal_ignore_case_key;

use super::*;
const EXCEL_MAX_ROWS: usize = 1_048_576;
const EXCEL_MAX_COLUMNS: usize = 16_384;
// Excel 数值单元格只保留 15 位有效数字，不能使用 JavaScript 的安全整数边界。
const EXCEL_INTEGER_MAX: i64 = 999_999_999_999_999;
const EXCEL_INTEGER_MIN: i64 = -999_999_999_999_999;
const EXCEL_UNSIGNED_INTEGER_MAX: u64 = 999_999_999_999_999;

/// 字典数据转模型
pub fn map_to_model<T>(data: &DataTableRow) -> Result<T, serde_json::Error>
where
    T: DataModel + Default,
{
    map_to_model_internal(data, false)
}

/// 字段名优先，未命中时按显示名称匹配；两种名称均区分大小写。
pub fn map_to_model_display_name<T: DataModel + Default>(
    data: &DataTableRow,
) -> Result<T, serde_json::Error> {
    map_to_model_internal(data, true)
}

fn map_to_model_internal<T: DataModel + Default>(
    data: &DataTableRow,
    display_names: bool,
) -> Result<T, serde_json::Error> {
    let mut model = T::default();
    for (column, value) in data {
        if value.is_null() {
            continue;
        }
        model
            .set_column(column, value, display_names)
            .map_err(|error| serde_json::Error::custom(format!("列 {column} 转换失败：{error}")))?;
    }
    Ok(model)
}

/// 模型转字典数据
pub fn model_to_map<T>(model: &T) -> Result<DataTableRow, serde_json::Error>
where
    T: PropertyModel,
{
    let mut row = IndexMap::new();
    for property in model.properties()? {
        row.insert(property.name.to_owned(), DataCell::from_property(property)?);
    }
    Ok(row)
}

/// 数据表转模型列表
pub fn data_table_to_list<T>(table: &DataTable) -> Result<Vec<T>, serde_json::Error>
where
    T: DataModel + Default,
{
    table.iter().map(map_to_model).collect()
}

pub fn data_table_to_list_display_name<T: DataModel + Default>(
    table: &DataTable,
) -> Result<Vec<T>, serde_json::Error> {
    table.iter().map(map_to_model_display_name).collect()
}

/// 模型列表转数据表
fn models_to_rows<T>(list: &[T]) -> Result<DataTable, serde_json::Error>
where
    T: PropertyModel,
{
    let mut table = None;
    for model in list.iter().filter(|model| !model.is_null_model()) {
        let properties = model.properties()?;
        if table.is_none() {
            table = Some(DataTable::new(
                T::model_name(),
                properties.iter().map(DataColumn::from_property).collect(),
            )?);
        }
        let mut row = DataTableRow::new();
        for property in properties {
            row.insert(property.name.to_owned(), DataCell::from_property(property)?);
        }
        table
            .as_mut()
            .expect("首个有效模型已建立列结构")
            .push_row(row)?;
    }
    match table {
        Some(table) => Ok(table),
        None => DataTable::new(T::model_name(), Vec::new()),
    }
}

pub fn list_to_data_table<T>(list: &[T]) -> Result<Option<DataTable>, serde_json::Error>
where
    T: PropertyModel,
{
    if list.is_empty() {
        return Ok(None);
    }

    models_to_rows(list).map(Some)
}

/// 将 Excel 导入为数据表
pub fn excel_to_data_table(
    file_path: &str,
    is_have_column_name: bool,
) -> Result<Option<DataTable>, String> {
    let reader = BufReader::new(File::open(file_path).map_err(|error| error.to_string())?);
    let extension = Path::new(file_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let mut workbook = match extension.to_ascii_lowercase().as_str() {
        "xlsx" => Sheets::Xlsx(Xlsx::new(reader).map_err(|error| error.to_string())?),
        "xls" => Sheets::Xls(Xls::new(reader).map_err(|error| error.to_string())?),
        _ => return Err("文件格式不支持，仅支持.xls和.xlsx格式".to_string()),
    };
    let Some(sheet_name) = workbook.sheet_names().first().cloned() else {
        return Ok(None);
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|error| error.to_string())?;

    let formulas = workbook
        .worksheet_formula(&sheet_name)
        .map_err(|error| error.to_string())?;
    let Some(last_column) = range
        .end()
        .into_iter()
        .chain(formulas.end())
        .map(|(_, column)| column)
        .max()
    else {
        return Ok(Some(DataTable::default()));
    };
    // 以工作表第 0 行为表头，不能把 calamine 裁剪后范围的首行误当作第 0 行。
    let Some(last_header_column) = (0..=last_column)
        .rev()
        .find(|&column| excel_position_has_value(&range, &formulas, (0, column)))
    else {
        return Ok(Some(DataTable::default()));
    };

    let mut source_columns = Vec::new();
    let mut headers = Vec::new();
    for column in 0..=last_header_column {
        let header = if is_have_column_name {
            // NPOI 的无求值器 DataFormatter 对公式表头使用公式文本。
            formulas
                .get_value((0, column))
                .filter(|formula| !formula.is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    range
                        .get_value((0, column))
                        .map(excel_header_text)
                        .unwrap_or_default()
                })
                .trim()
                .to_string()
        } else {
            format!("column{}", column + 1)
        };
        if !header.is_empty() {
            headers.push(header);
            source_columns.push(column);
        }
    }
    validate_unique_headers(&headers)?;

    let mut table = DataTable::new(
        "",
        headers
            .iter()
            .map(|name| DataColumn::new(name, DataColumnType::Object))
            .collect(),
    )
    .map_err(|error| error.to_string())?;
    let last_row = range
        .end()
        .into_iter()
        .chain(formulas.end())
        .map(|(row, _)| row)
        .max()
        .unwrap_or(0);
    for row in u32::from(is_have_column_name)..=last_row {
        if !source_columns
            .iter()
            .any(|&column| excel_position_has_value(&range, &formulas, (row, column)))
        {
            continue;
        }

        let mut item = IndexMap::new();
        for (&column, header) in source_columns.iter().zip(&headers) {
            let value = range
                .get_value((row, column))
                .map(cell_to_value)
                .unwrap_or_else(|| DataCell::Text(String::new()));
            item.insert(header.clone(), value);
        }
        table.push_row(item).map_err(|error| error.to_string())?;
    }

    Ok(Some(table))
}

/// 将列表导出为 Excel 文件流
pub fn list_to_excel<T>(list: &[T]) -> Result<Vec<u8>, String>
where
    T: PropertyModel + Default,
{
    list_to_excel_internal(list, false)
}

/// 使用字段显示名称导出表头；未声明显示名称的字段使用属性名。
pub fn list_to_excel_display_name<T>(list: &[T]) -> Result<Vec<u8>, String>
where
    T: PropertyModel + Default,
{
    list_to_excel_internal(list, true)
}

/// 将列表按模板导出为 Excel 文件流
pub fn list_to_excel_with_template<T>(
    list: &[T],
    excel_template: &ExcelTemplate,
) -> Result<Vec<u8>, String>
where
    T: PropertyModel,
{
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    for (column_index, column) in excel_template.column_list.iter().enumerate() {
        let column_index = excel_column_index(column_index)?;
        worksheet
            .write_string(0, column_index, &column.title)
            .map_err(|error| error.to_string())?;
    }

    for (row_index, item) in list.iter().enumerate() {
        let row_index = excel_row_index(row_index + 1)?;
        for (column_index, column) in excel_template.column_list.iter().enumerate() {
            let column_index = excel_column_index(column_index)?;
            let property = get_property(item, &column.field).map_err(|error| error.to_string())?;
            write_excel_property(worksheet, row_index, column_index, &property, false)?;
        }
    }

    workbook.save_to_buffer().map_err(|error| error.to_string())
}

/// 获取字典中的字符串值
pub fn get_string_value<'a>(row: &'a DataTableRow, key: &str) -> Option<&'a str> {
    row.get(key).and_then(DataCell::as_str)
}

/// 获取字典中的整数值
pub fn get_i64_value(row: &DataTableRow, key: &str) -> Option<i64> {
    row.get(key).and_then(DataCell::as_i64)
}

/// Excel 导出模板
#[derive(Debug, Clone, Default, Serialize, Deserialize, PropertyModel)]
pub struct ExcelTemplate {
    /// 列集合
    #[property(name = "ColumnList")]
    pub column_list: Vec<ColumnProperty>,
}

/// Excel 导出列定义
#[derive(Debug, Clone, Default, Serialize, Deserialize, PropertyModel, DataModel)]
pub struct ColumnProperty {
    /// 标题
    #[property(name = "Title")]
    pub title: String,

    /// 字段名
    #[property(name = "Field")]
    pub field: String,
}

/// 列表导出 Excel
fn list_to_excel_internal<T>(list: &[T], display_names: bool) -> Result<Vec<u8>, String>
where
    T: PropertyModel + Default,
{
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // 表头来自默认模型，空列表也保留列；不从数据行汇总或排序字段。
    let model = T::default();
    let headers = if display_names {
        get_properties_display_name(&model)
    } else {
        get_properties(&model)
    }
    .map_err(|error| error.to_string())?;

    for (index, header) in headers.keys().enumerate() {
        let index = excel_column_index(index)?;
        worksheet
            .write_string(0, index, header)
            .map_err(|error| error.to_string())?;
    }

    for (row_index, item) in list.iter().enumerate() {
        let row_index = excel_row_index(row_index + 1)?;
        let properties = item.properties().map_err(|error| error.to_string())?;
        for (column_index, property) in properties
            .iter()
            .filter(|property| property.kind != PropertyKind::Reference)
            .enumerate()
        {
            let column_index = excel_column_index(column_index)?;
            write_excel_property(worksheet, row_index, column_index, property, true)?;
        }
    }

    workbook.save_to_buffer().map_err(|error| error.to_string())
}

fn write_excel_property(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    column: u16,
    property: &Property,
    property_dictionary: bool,
) -> Result<(), String> {
    let value = if property.value.is_null() {
        Value::Null
    } else {
        match property.kind {
            PropertyKind::Enum => Value::String(
                property
                    .display_value()
                    .map_err(|error| error.to_string())?,
            ),
            PropertyKind::DateTime if property.nullable || !property_dictionary => Value::String(
                property
                    .date_time()
                    .map_err(|error| error.to_string())?
                    .format("%Y/%m/%d %H:%M:%S")
                    .to_string(),
            ),
            PropertyKind::DateTimeOffset => Value::String(
                property
                    .offset_time()
                    .map_err(|error| error.to_string())?
                    .with_timezone(&chrono_tz::Asia::Shanghai)
                    .format("%Y/%m/%d %H:%M:%S")
                    .to_string(),
            ),
            PropertyKind::Decimal => {
                let decimal = property.decimal().map_err(|error| error.to_string())?;
                let text = decimal.to_string();
                let digits = text.trim_start_matches('-').replace('.', "");
                if digits.trim_start_matches('0').trim_end_matches('0').len() <= 15 {
                    Value::from(decimal.to_f64().ok_or("decimal 无法转换为 Excel 数值")?)
                } else {
                    Value::String(text)
                }
            }
            PropertyKind::Value => Value::String(match &property.value {
                Value::String(value) => value.clone(),
                value => value.to_string(),
            }),
            PropertyKind::Reference => property.value.clone(),
            _ => property
                .scalar_value()
                .map_err(|error| error.to_string())?
                .unwrap_or(Value::Null),
        }
    };
    write_excel_value(worksheet, row, column, &value)
}

/// 写入 Excel 单元格
fn write_excel_value(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &Value,
) -> Result<(), String> {
    match value {
        Value::Null => worksheet
            .write_string(row, col, "")
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Value::Bool(value) => worksheet
            .write_boolean(row, col, *value)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Value::Number(value) => {
            if value
                .as_i64()
                .is_some_and(|item| !(EXCEL_INTEGER_MIN..=EXCEL_INTEGER_MAX).contains(&item))
                || value
                    .as_u64()
                    .is_some_and(|item| item > EXCEL_UNSIGNED_INTEGER_MAX)
            {
                return worksheet
                    .write_string(row, col, value.to_string())
                    .map(|_| ())
                    .map_err(|error| error.to_string());
            }

            if let Some(number) = value.as_f64() {
                worksheet
                    .write_number(row, col, number)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            } else {
                worksheet
                    .write_string(row, col, value.to_string())
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        }
        Value::String(value) => worksheet
            .write_string(row, col, value)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Value::Array(_) | Value::Object(_) => worksheet
            .write_string(row, col, value.to_string())
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
}

/// 校验表头不能重复
fn validate_unique_headers(headers: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for header in headers {
        if !seen.insert(ordinal_ignore_case_key(header)) {
            return Err(format!("Excel表头存在重复列：{header}"));
        }
    }

    Ok(())
}

/// 校验 Excel 行索引
fn excel_row_index(index: usize) -> Result<u32, String> {
    if index >= EXCEL_MAX_ROWS {
        return Err(format!("Excel 行数不能超过 {EXCEL_MAX_ROWS}"));
    }

    Ok(index as u32)
}

/// 校验 Excel 列索引
fn excel_column_index(index: usize) -> Result<u16, String> {
    if index >= EXCEL_MAX_COLUMNS {
        return Err(format!("Excel 列数不能超过 {EXCEL_MAX_COLUMNS}"));
    }

    Ok(index as u16)
}

/// 不依赖区域格式的基础表头文本。
fn excel_header_text(cell: &Data) -> String {
    match cell {
        Data::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
        _ => cell.to_string(),
    }
}

/// Excel 单元格保留数值和日期类型；缺失、错误与空白仍为空字符串。
fn cell_to_value(cell: &Data) -> DataCell {
    match cell {
        Data::Empty => DataCell::Text(String::new()),
        Data::String(value) => DataCell::Text(value.clone()),
        Data::Float(value) => DataCell::Float(*value),
        Data::Int(value) => DataCell::Integer(*value),
        Data::Bool(value) => DataCell::Boolean(*value),
        Data::DateTime(value) => {
            let (year, month, day, hour, minute, second, millis) = value.to_ymd_hms_milli();
            NaiveDate::from_ymd_opt(i32::from(year), u32::from(month), u32::from(day))
                .and_then(|date| {
                    date.and_hms_milli_opt(
                        u32::from(hour),
                        u32::from(minute),
                        u32::from(second),
                        u32::from(millis),
                    )
                })
                .map(DataCell::DateTime)
                .unwrap_or_else(|| DataCell::Text(String::new()))
        }
        Data::DateTimeIso(value) => value
            .parse()
            .map(DataCell::DateTime)
            .unwrap_or_else(|_| DataCell::Text(value.clone())),
        Data::DurationIso(value) => DataCell::Text(value.clone()),
        Data::Error(_) => DataCell::Text(String::new()),
    }
}

/// 公式即使缓存结果为空也占用一格，空行判断只检查实际导入的列。
fn excel_position_has_value(
    values: &Range<Data>,
    formulas: &Range<String>,
    position: (u32, u32),
) -> bool {
    formulas
        .get_value(position)
        .is_some_and(|formula| !formula.trim().is_empty())
        || values.get_value(position).is_some_and(excel_cell_has_value)
}

/// 判断 Excel 单元格是否包含有效数据
fn excel_cell_has_value(cell: &Data) -> bool {
    match cell {
        Data::Empty => false,
        Data::String(value) => !value.trim().is_empty(),
        _ => true,
    }
}
