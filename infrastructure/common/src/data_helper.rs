use calamine::{Data, Reader, open_workbook_auto};
use rust_xlsxwriter::Workbook;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

const EXCEL_MAX_ROWS: usize = 1_048_576;
const EXCEL_MAX_COLUMNS: usize = 16_384;
const JS_SAFE_INTEGER_MAX: i64 = 9_007_199_254_740_991;
const JS_SAFE_INTEGER_MIN: i64 = -9_007_199_254_740_991;
const JS_SAFE_UNSIGNED_INTEGER_MAX: u64 = 9_007_199_254_740_991;

/// 数据表行
pub type DataTableRow = HashMap<String, Value>;

/// 数据表
pub type DataTable = Vec<DataTableRow>;

/// 字典数据转模型
pub fn map_to_model<T>(data: &DataTableRow) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    let map = data
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<String, Value>>();

    serde_json::from_value(Value::Object(map))
}

/// 模型转字典数据
pub fn model_to_map<T>(model: &T) -> Result<DataTableRow, serde_json::Error>
where
    T: Serialize,
{
    let value = serde_json::to_value(model)?;

    match value {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Ok(HashMap::new()),
    }
}

/// 数据表转模型列表
pub fn data_table_to_list<T>(table: &DataTable) -> Result<Vec<T>, serde_json::Error>
where
    T: DeserializeOwned,
{
    table.iter().map(map_to_model).collect()
}

/// 模型列表转数据表
fn models_to_rows<T>(list: &[T]) -> Result<DataTable, serde_json::Error>
where
    T: Serialize,
{
    list.iter().map(model_to_map).collect()
}

pub fn list_to_data_table<T>(list: &[T]) -> Result<Option<DataTable>, serde_json::Error>
where
    T: Serialize,
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
    let mut workbook = open_workbook_auto(file_path).map_err(|error| error.to_string())?;
    let Some(sheet_name) = workbook.sheet_names().first().cloned() else {
        return Ok(None);
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|error| error.to_string())?;

    let mut rows = range.rows();
    let Some(first_row) = rows.next() else {
        return Ok(None);
    };

    let headers = if is_have_column_name {
        first_row
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let text = cell.to_string();
                if text.trim().is_empty() {
                    format!("column{}", index + 1)
                } else {
                    text.trim().to_string()
                }
            })
            .collect::<Vec<_>>()
    } else {
        (0..first_row.len())
            .map(|index| format!("column{}", index + 1))
            .collect::<Vec<_>>()
    };
    validate_unique_headers(&headers)?;

    let data_rows = if is_have_column_name {
        rows.collect::<Vec<_>>()
    } else {
        let mut all_rows = vec![first_row];
        all_rows.extend(rows);
        all_rows
    };

    let mut table = Vec::new();
    for row in data_rows {
        let mut item = HashMap::new();
        for (index, header) in headers.iter().enumerate() {
            let value = row.get(index).map(cell_to_json).unwrap_or(Value::Null);
            item.insert(header.clone(), value);
        }
        table.push(item);
    }

    Ok(Some(table))
}

/// 将列表导出为 Excel 文件流
pub fn list_to_excel<T>(list: &[T]) -> Result<Vec<u8>, String>
where
    T: Serialize,
{
    list_to_excel_internal(list, None)
}

/// 将列表按模板导出为 Excel 文件流
pub fn list_to_excel_with_template<T>(
    list: &[T],
    excel_template: &ExcelTemplate,
) -> Result<Vec<u8>, String>
where
    T: Serialize,
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
        let value = serde_json::to_value(item).map_err(|error| error.to_string())?;
        for (column_index, column) in excel_template.column_list.iter().enumerate() {
            let column_index = excel_column_index(column_index)?;
            let cell_value = extract_nested_value(&value, &column.field).unwrap_or(Value::Null);
            write_excel_value(worksheet, row_index, column_index, &cell_value)?;
        }
    }

    workbook.save_to_buffer().map_err(|error| error.to_string())
}

/// 获取字典中的字符串值
pub fn get_string_value<'a>(row: &'a DataTableRow, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

/// 获取字典中的整数值
pub fn get_i64_value(row: &DataTableRow, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

/// Excel 导出模板
#[derive(Debug, Clone)]
pub struct ExcelTemplate {
    /// 列集合
    pub column_list: Vec<ColumnProperty>,
}

/// Excel 导出列定义
#[derive(Debug, Clone)]
pub struct ColumnProperty {
    /// 标题
    pub title: String,

    /// 字段名
    pub field: String,
}

/// 列表导出 Excel
fn list_to_excel_internal<T>(list: &[T], headers: Option<&[String]>) -> Result<Vec<u8>, String>
where
    T: Serialize,
{
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    let rows = list
        .iter()
        .map(model_to_map::<T>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let header_list = if let Some(headers) = headers {
        headers.to_vec()
    } else {
        let mut headers = rows
            .iter()
            .flat_map(|row| row.keys().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        headers.sort();
        headers
    };

    for (index, header) in header_list.iter().enumerate() {
        let index = excel_column_index(index)?;
        worksheet
            .write_string(0, index, header)
            .map_err(|error| error.to_string())?;
    }

    for (row_index, row) in rows.iter().enumerate() {
        let row_index = excel_row_index(row_index + 1)?;
        for (column_index, header) in header_list.iter().enumerate() {
            let column_index = excel_column_index(column_index)?;
            let value = row.get(header).cloned().unwrap_or(Value::Null);
            write_excel_value(worksheet, row_index, column_index, &value)?;
        }
    }

    workbook.save_to_buffer().map_err(|error| error.to_string())
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
                .is_some_and(|item| !(JS_SAFE_INTEGER_MIN..=JS_SAFE_INTEGER_MAX).contains(&item))
                || value
                    .as_u64()
                    .is_some_and(|item| item > JS_SAFE_UNSIGNED_INTEGER_MAX)
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

/// 从嵌套对象读取字段
fn extract_nested_value(value: &Value, field: &str) -> Option<Value> {
    let mut current = value;

    for segment in field.split('.') {
        current = current.get(segment)?;
    }

    Some(current.clone())
}

/// 校验表头不能重复
fn validate_unique_headers(headers: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for header in headers {
        if !seen.insert(header) {
            return Err(format!("Excel 表头重复: {header}"));
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

/// Excel 单元格转 Json
fn cell_to_json(cell: &Data) -> Value {
    match cell {
        Data::Empty => Value::Null,
        Data::String(value) => Value::String(value.clone()),
        Data::Float(value) => Value::from(*value),
        Data::Int(value) => Value::from(*value),
        Data::Bool(value) => Value::from(*value),
        Data::DateTime(value) => Value::String(value.to_string()),
        Data::DateTimeIso(value) => Value::String(value.clone()),
        Data::DurationIso(value) => Value::String(value.clone()),
        Data::Error(error) => Value::String(error.to_string()),
    }
}
