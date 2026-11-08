use common::app_error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 默认页码
pub const PAGE_DEFAULT: i32 = 1;

/// 默认每页条数
pub const PAGE_SIZE_DEFAULT: i32 = 20;

/// 每页条数上限
pub const PAGE_SIZE_MAX: i32 = 100;

/// 通用分页返回结构
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageListDto<T: Serialize + ToSchema> {
    /// 总记录数
    pub total: i32,

    /// 当前页数据列表；未查询列表时为 null，已查询但无本页数据时为空数组。
    pub list: Option<Vec<T>>,
}

/// 通用分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct PageRequestDto {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,
}

/// 默认页码
pub fn default_page() -> i32 {
    PAGE_DEFAULT
}

/// 默认每页条数
pub fn default_page_size() -> i32 {
    PAGE_SIZE_DEFAULT
}

/// 校验每页条数范围；页码交由偏移计算和查询层处理。
pub fn validate_page_request(_page: i32, page_size: i32) -> Result<(), AppError> {
    if page_size <= 0 {
        return Err(AppError::bad_request("pageSize 必须大于 0"));
    }

    if page_size > PAGE_SIZE_MAX {
        return Err(AppError::bad_request(format!(
            "pageSize 不可以大于 {PAGE_SIZE_MAX}"
        )));
    }

    Ok(())
}

/// 对应源 PageRequestDto.Skip 的 Int32 运算；Debug 与 Release 保持相同行为。
pub fn page_offset(page: i32, page_size: i32) -> i32 {
    page.wrapping_sub(1).wrapping_mul(page_size)
}

/// 通用分页请求约定
pub trait PageRequest: common::json_helper::input::QueryModel {
    /// 对应宿主声明的复杂 Query 参数名，支持整体选择带前缀的属性。
    const QUERY_PARAMETER_NAME: &'static str = "request";

    /// 与分页对象一同接收的独立参数，不参与对象前缀选择。
    const QUERY_PARAMETERS: &'static [&'static str] = &[];

    /// 分页模型额外的 Query 属性名；错误显示名沿源模型声明，不从 JSON 大小写猜测。
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] = &[];

    /// 获取页码
    fn page(&self) -> i32;

    /// 获取每页条数
    fn page_size(&self) -> i32;
}

/// Query 字段绑定成功后仍参与验证，不能因其他属性转换失败而跳过范围校验。
pub fn validate_page_size_query(value: &i32) -> Result<(), common::json_helper::input::InputError> {
    if (1..=PAGE_SIZE_MAX).contains(value) {
        Ok(())
    } else {
        Err(common::json_helper::input::InputError::range(
            "pageSize",
            1,
            i64::from(PAGE_SIZE_MAX),
        ))
    }
}

impl PageRequest for PageRequestDto {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}
