use common::json_helper::deserialize_option_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 远程上传文件请求
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUploadFileDto {
    /// 业务领域
    pub business: String,

    /// 关联业务主键
    #[serde(rename = "key")]
    #[serde(
        default,
        deserialize_with = "deserialize_option_i64_from_string_or_number"
    )]
    pub key: Option<i64>,

    /// 自定义标记
    #[serde(default)]
    pub sign: String,

    /// 是否允许公开访问
    #[serde(rename = "isPublicRead", default)]
    pub is_public_read: bool,

    /// 文件名称
    pub file_name: String,

    /// 远程文件地址
    #[serde(rename = "fileUrl")]
    pub url: String,
}

/// 应用层上传文件请求
pub struct UploadFileDto {
    /// 业务领域
    pub business: String,

    /// 关联业务主键
    pub key: Option<i64>,

    /// 自定义标记
    pub sign: String,

    /// 是否允许公开访问
    pub is_public_read: bool,

    /// 文件名称
    pub file_name: String,

    /// 临时文件路径
    pub temp_file_path: String,
}

/// 文件信息返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoDto {
    /// 文件 ID
    pub id: String,

    /// 文件名称
    pub name: String,

    /// 文件大小
    pub length: i64,

    /// 文件大小文本
    pub length_text: String,

    /// 文件相对路径
    pub path: String,

    /// 自定义标记
    pub sign: String,

    /// 文件访问地址
    pub url: Option<String>,
}

/// 文件下载内容
pub struct FileContentDto {
    /// 文件名称
    pub file_name: String,

    /// 文件相对路径
    pub path: String,

    /// 文件内容
    pub bytes: Vec<u8>,
}

/// 文件本地读取信息
pub struct FileLocalContentDto {
    /// 文件名称
    pub file_name: String,

    /// 文件相对路径
    pub path: String,

    /// 本地物理路径
    pub local_path: std::path::PathBuf,
}
