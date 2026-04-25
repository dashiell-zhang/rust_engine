use application_interface::error::AppError;
use serde::Serialize;
use utoipa::ToSchema;

/// 最小页码
pub const PAGE_MIN: u64 = 1;

/// 默认每页条数
pub const PAGE_SIZE_DEFAULT: u64 = 20;

/// 每页条数上限
pub const PAGE_SIZE_MAX: u64 = 100;

/// 通用分页返回结构
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageListDto<T: Serialize + ToSchema> {
    /// 总记录数
    pub total: u64,

    /// 当前页数据列表
    pub list: Vec<T>,
}

/// 默认页码
pub fn default_page() -> u64 {
    PAGE_MIN
}

/// 默认每页条数
pub fn default_page_size() -> u64 {
    PAGE_SIZE_DEFAULT
}

/// 校验页码和每页条数
pub fn validate_page_request(page: u64, page_size: u64) -> Result<(), AppError> {
    if page_size == 0 {
        return Err(AppError::bad_request("pageSize 必须大于 0"));
    }

    if page_size > PAGE_SIZE_MAX {
        return Err(AppError::bad_request(format!(
            "pageSize 不可以大于 {PAGE_SIZE_MAX}"
        )));
    }

    if page < PAGE_MIN {
        return Err(AppError::bad_request("page 必须大于 0"));
    }

    Ok(())
}

/// 规范化页码
pub fn normalize_page(page: u64) -> u64 {
    page.max(PAGE_MIN)
}

/// 通用分页请求约定
pub trait PageRequest {
    /// 获取页码
    fn page(&self) -> u64;

    /// 获取每页条数
    fn page_size(&self) -> u64;
}
