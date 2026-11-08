use common::json_helper::input::ProtocolModel;
use indexmap::IndexMap;
use serde::Serialize;
use utoipa::ToSchema;

/// 邮件发送参数；队列采用源模型的属性名称，引用字段允许为空。
#[derive(Debug, Clone, Default, Serialize, ProtocolModel, ToSchema)]
#[serde(default, rename_all = "PascalCase")]
#[protocol(case_sensitive)]
pub struct SendEmailDto {
    /// 发件人显示名称
    pub from_display_name: Option<String>,
    /// 收件人地址列表
    pub to_addresses: Option<Vec<Option<String>>>,
    /// 邮件标题
    pub subject: Option<String>,
    /// 邮件正文
    pub body: Option<String>,
    /// 正文是否为 HTML 格式
    pub is_body_html: bool,
    /// 附件文件 ID 列表
    pub file_id_list: Option<Vec<i64>>,
}

impl SendEmailDto {
    /// 使用源队列的对象和列表引用元数据，长整数交给公共克隆编码。
    pub fn to_queue_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct List<'a, T> {
            #[serde(rename = "$id")]
            id: &'static str,
            #[serde(rename = "$values")]
            values: &'a Vec<T>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Message<'a> {
            #[serde(rename = "$id")]
            id: &'static str,
            from_display_name: &'a Option<String>,
            to_addresses: Option<List<'a, Option<String>>>,
            subject: &'a Option<String>,
            body: &'a Option<String>,
            is_body_html: bool,
            file_id_list: Option<List<'a, i64>>,
        }
        common::json_helper::object_clone_json(&Message {
            id: "1",
            from_display_name: &self.from_display_name,
            to_addresses: self
                .to_addresses
                .as_ref()
                .map(|values| List { id: "2", values }),
            subject: &self.subject,
            body: &self.body,
            is_body_html: self.is_body_html,
            file_id_list: self.file_id_list.as_ref().map(|values| List {
                id: if self.to_addresses.is_some() {
                    "3"
                } else {
                    "2"
                },
                values,
            }),
        })
    }
}

/// 短信发送任务参数；引用字段缺失或为 null 时保留为空。
#[derive(Debug, Clone, Default, Serialize, ProtocolModel, ToSchema)]
#[serde(default, rename_all = "PascalCase")]
#[protocol(case_sensitive)]
pub struct SendSmsDto {
    /// 短信签名
    pub sign_name: Option<String>,
    /// 接收手机号
    pub phone: Option<String>,
    /// 模板编号
    pub template_code: Option<String>,
    /// 模板参数；保留字典顺序、键名及可空值。
    #[schema(value_type = Object)]
    pub template_params: Option<IndexMap<String, Option<String>>>,
}

impl SendSmsDto {
    /// 生成源 ObjectCloneJson 的对象及字典引用元数据。
    pub fn to_queue_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct Parameters<'a> {
            #[serde(rename = "$id")]
            id: &'static str,
            #[serde(flatten)]
            values: &'a IndexMap<String, Option<String>>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Message<'a> {
            #[serde(rename = "$id")]
            id: &'static str,
            sign_name: &'a Option<String>,
            phone: &'a Option<String>,
            template_code: &'a Option<String>,
            template_params: Option<Parameters<'a>>,
        }
        common::json_helper::object_clone_json(&Message {
            id: "1",
            sign_name: &self.sign_name,
            phone: &self.phone,
            template_code: &self.template_code,
            template_params: self
                .template_params
                .as_ref()
                .map(|values| Parameters { id: "2", values }),
        })
    }
}
