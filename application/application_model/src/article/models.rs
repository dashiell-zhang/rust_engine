use crate::file::models::FileInfoDto;
use crate::shared::models::{PageListDto, default_page, default_page_size};
use common::json_helper::deserialize_option_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 栏目分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct CategoryPageRequest {
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

impl crate::shared::models::PageRequest for CategoryPageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 栏目 ID 查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct CategoryIdQuery {
    /// 栏目 ID
    #[serde(rename = "categoryId", default)]
    pub category_id: i64,
}

/// 删除栏目查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteCategoryQuery {
    /// 栏目 ID
    #[serde(default)]
    pub id: i64,
}

/// 栏目信息返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    /// 栏目 ID
    pub id: String,

    /// 栏目名称
    pub name: String,

    /// 备注
    pub remark: Option<String>,

    /// 排序值
    pub sort: i32,

    /// 父级栏目 ID
    pub parent_id: Option<String>,

    /// 父级栏目名称
    pub parent_name: Option<String>,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,
}

/// 栏目树形选择项
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategorySelectDto {
    /// 栏目 ID
    pub id: String,

    /// 栏目名称
    pub name: String,

    /// 子栏目列表
    #[schema(no_recursion)]
    pub child_list: Vec<CategorySelectDto>,
}

/// 编辑栏目请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Site.Article.EditCategoryDto")]
pub struct EditCategoryDto {
    /// 栏目名称
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 父级栏目 ID
    #[serde(
        default,
        deserialize_with = "deserialize_option_i64_from_string_or_number"
    )]
    pub parent_id: Option<i64>,

    /// 备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub remark: Option<String>,

    /// 排序值
    #[serde(default)]
    pub sort: i32,
}

/// 栏目分页列表返回模型
pub type CategoryPageListDto = PageListDto<CategoryDto>;

/// 文章 ID 查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct ArticleIdQuery {
    /// 文章 ID
    #[serde(rename = "articleId", default)]
    pub article_id: i64,
}

/// 删除文章查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteArticleQuery {
    /// 文章 ID
    #[serde(default)]
    pub id: i64,
}

/// 创建文章查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct CreateArticleQuery {
    /// 本次文章编辑上传批次标识
    #[serde(rename = "uploadKey", default)]
    pub upload_key: i64,
}

/// 更新文章查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UpdateArticleQuery {
    /// 文章 ID
    #[serde(rename = "articleId", default)]
    pub article_id: i64,

    /// 本次文章编辑上传批次标识
    #[serde(rename = "uploadKey", default)]
    pub upload_key: i64,
}

/// 文章信息模型，同时用于演示服务的文章参数绑定。
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Site.Article.ArticleDto")]
pub struct ArticleDto {
    /// 文章 ID
    #[serde(
        default = "crate::shared::input::zero_id",
        deserialize_with = "crate::shared::input::id"
    )]
    #[json(value, scalar_type = "System.Int64")]
    pub id: String,

    /// 所属栏目 ID
    #[serde(
        default = "crate::shared::input::zero_id",
        deserialize_with = "crate::shared::input::id"
    )]
    #[json(value, scalar_type = "System.Int64")]
    pub category_id: String,

    /// 所属栏目名称
    #[serde(deserialize_with = "crate::shared::input::required_string")]
    #[json(required)]
    pub category_name: Option<String>,

    /// 标题
    #[json(required = "标题不可以空")]
    pub title: String,

    /// 正文内容
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub content: Option<String>,

    /// 是否推荐
    #[serde(default)]
    pub is_recommend: bool,

    /// 是否显示
    #[serde(default)]
    pub is_display: bool,

    /// 排序值
    #[serde(default)]
    pub sort: i32,

    /// 点击量
    #[serde(default)]
    pub click_count: i32,

    /// 摘要
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub digest: Option<String>,

    /// 创建时间
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "crate::shared::input::date_time"
    )]
    #[json(value, scalar_type = "System.DateTimeOffset")]
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,

    /// 封面文件列表
    #[json(nested)]
    pub cover_image_list: Option<Vec<FileInfoDto>>,
}

/// 编辑文章请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Site.Article.EditArticleDto")]
pub struct EditArticleDto {
    /// 所属栏目 ID
    #[json(required = "所属栏目不可以空")]
    pub category_id: String,

    /// 标题
    #[json(required = "标题不可以空")]
    pub title: String,

    /// 正文内容
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub content: Option<String>,

    /// 正文当前引用的统一文件 ID 列表
    #[serde(default)]
    pub content_file_id_list: Vec<i64>,

    /// 摘要，留空时由正文自动生成
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub digest: Option<String>,

    /// 是否推荐
    #[serde(default)]
    pub is_recommend: bool,

    /// 是否显示
    #[serde(default)]
    pub is_display: bool,

    /// 排序值
    #[serde(default)]
    pub sort: i32,

    /// 点击量
    #[serde(default)]
    pub click_count: i32,
}

/// 文章分页列表返回模型
pub type ArticlePageListDto = PageListDto<ArticleDto>;

/// 客户端文章分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct PublicArticlePageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 按栏目筛选
    #[serde(rename = "categoryId")]
    pub category_id: Option<i64>,

    /// 是否包含子栏目文章
    #[serde(default = "default_include_children")]
    pub include_children: bool,

    /// 关键词搜索
    pub keyword: Option<String>,
}

impl crate::shared::models::PageRequest for PublicArticlePageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 客户端文章列表项
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicArticleListItemDto {
    /// 文章 ID
    pub id: String,

    /// 所属栏目 ID
    pub category_id: String,

    /// 所属栏目名称
    pub category_name: Option<String>,

    /// 标题
    pub title: String,

    /// 摘要
    pub digest: Option<String>,

    /// 是否推荐
    pub is_recommend: bool,

    /// 点击量
    pub click_count: i32,

    /// 创建时间
    pub create_time: String,
}

/// 客户端文章详情
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicArticleDetailDto {
    /// 文章 ID
    pub id: String,

    /// 所属栏目 ID
    pub category_id: String,

    /// 所属栏目名称
    pub category_name: Option<String>,

    /// 标题
    pub title: String,

    /// 正文内容
    pub content: Option<String>,

    /// 摘要
    pub digest: Option<String>,

    /// 是否推荐
    pub is_recommend: bool,

    /// 点击量
    pub click_count: i32,

    /// 创建时间
    pub create_time: String,
}

/// 客户端文章分页返回模型
pub type PublicArticlePageListDto = PageListDto<PublicArticleListItemDto>;

/// 客户端公开栏目节点
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicCategoryDto {
    /// 栏目 ID
    pub id: String,

    /// 栏目名称
    pub name: String,

    /// 子栏目列表
    #[schema(no_recursion)]
    pub child_list: Vec<PublicCategoryDto>,
}

/// 客户端推荐文章查询参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PublicRecommendArticleRequest {
    /// 返回条数
    #[serde(default = "default_public_recommend_take")]
    pub take: u64,
}

/// 文章点击量更新结果
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArticleClickDto {
    /// 文章 ID
    pub id: String,

    /// 最新点击量
    pub click_count: i32,
}

/// 客户端相邻文章信息
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicAdjacentArticleDto {
    /// 文章 ID
    pub id: String,

    /// 标题
    pub title: String,
}

/// 客户端文章详情导航信息
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicArticleNavigationDto {
    /// 上一篇文章
    pub previous: Option<PublicAdjacentArticleDto>,

    /// 下一篇文章
    pub next: Option<PublicAdjacentArticleDto>,
}

/// 客户端推荐文章默认返回条数
fn default_public_recommend_take() -> u64 {
    10
}

/// 默认包含子栏目文章
fn default_include_children() -> bool {
    true
}
