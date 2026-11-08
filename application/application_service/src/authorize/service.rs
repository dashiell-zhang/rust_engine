#![allow(clippy::collapsible_if)]

use repository_core::versioned::VersionedActiveModel;

use async_recursion::async_recursion;
use chrono::{Duration, Utc};
use common::{
    crypto_helper::{hash_password_pbkdf2_sha256, verify_password_pbkdf2_sha256},
    string_helper::trim_prefix_name,
};
use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
use indexmap::IndexMap;
use repository::extensions::{SoftDeleteQueryExt, SoftDeleteSelectExt};
use repository::{
    app_setting, function, function_authorize, function_route, user, user_bind_external, user_role,
    user_token,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel,
    QueryFilter, QueryOrder, Set, TransactionTrait, prelude::DateTimeWithTimeZone,
    sea_query::Condition,
};
use serde_json::json;
use std::collections::HashMap;

use crate::{
    task_center::{models::CreateQueueTaskDto, service::TaskCenterService},
    write_support::{build_system_create_audit_fields, write_data_update_log},
};
use application_interface::error::AppError;
use application_model::{
    authorize::models::{
        FunctionRouteDto, FunctionTreeDto, GetFunctionRouteListRequest, GetTokenBySmsRequest,
        GetTokenByWxAppRequest, GetTokenByWxMiniAppRequest, GetTokenRequest,
        GetWeiXinAppUserInfoDto, JwtClaims, SendSmsVerifyCodeRequest, SetFunctionRouteDto,
        UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
    },
    message::models::SendSmsDto,
};
use application_runtime::context::{AuthorizeContext, CurrentUser};

/// 模块类型
const FUNCTION_TYPE_MODULE: i32 = 1;

/// 功能类型
const FUNCTION_TYPE_FUNCTION: i32 = 2;

/// 授权模块应用服务
pub struct AuthorizeService<'a> {
    /// 授权模块上下文
    pub(super) context: AuthorizeContext<'a>,
}

impl<'a> AuthorizeService<'a> {
    /// 创建授权模块应用服务
    pub fn new(context: AuthorizeContext<'a>) -> Self {
        Self { context }
    }

    /// 按配置原样返回 RSA 公钥，不使用 JWT 公钥或派生私钥内容作为回退。
    pub fn get_public_key(&self) -> Result<Option<String>, AppError> {
        self.context
            .rsa_public_key
            .map(|key| key.map(str::to_owned))
            .ok_or_else(|| AppError::bad_request("RSA公钥加载异常"))
    }

    /// 校验登录请求
    fn validate_get_token_request(request: &GetTokenRequest) -> Result<(), AppError> {
        if request.user_name.trim().is_empty() {
            return Err(AppError::bad_request("用户名不可以空"));
        }

        if request.password.is_empty() {
            return Err(AppError::bad_request("密码不可以为空"));
        }

        Ok(())
    }

    /// 校验短信登录请求
    fn validate_get_token_by_sms_request(request: &GetTokenBySmsRequest) -> Result<(), AppError> {
        if request.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        if request.verify_code.trim().is_empty() {
            return Err(AppError::bad_request("短信验证码不可以空"));
        }

        Ok(())
    }

    /// 校验短信验证码发送请求
    fn validate_send_sms_verify_code_request(
        request: &SendSmsVerifyCodeRequest,
    ) -> Result<(), AppError> {
        if request.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        Ok(())
    }

    /// 校验短信修改密码请求
    fn validate_update_password_by_sms_request(
        request: &UpdatePasswordBySmsRequest,
    ) -> Result<(), AppError> {
        if request.sms_code.trim().is_empty() {
            return Err(AppError::bad_request("短信验证码不可以空"));
        }

        if request.new_password.is_empty() {
            return Err(AppError::bad_request("新密码不可以空"));
        }

        Ok(())
    }

    /// 校验旧密码修改密码请求
    fn validate_update_password_by_old_password_request(
        request: &UpdatePasswordByOldPasswordRequest,
    ) -> Result<(), AppError> {
        if request.old_password.is_empty() {
            return Err(AppError::bad_request("旧密码不可以空"));
        }

        if request.new_password.is_empty() {
            return Err(AppError::bad_request("新密码不可以空"));
        }

        Ok(())
    }

    /// 用户名密码登录并签发 Token
    pub async fn get_token(&self, request: GetTokenRequest) -> Result<String, AppError> {
        Self::validate_get_token_request(&request)?;

        let user_models = user::Entity::find()
            .filter(user::Column::UserName.eq(&request.user_name))
            .not_deleted(user::Column::DeleteTime)
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户查询失败: {error}")))?;

        let Some(user_model) = user_models.into_iter().find(|user_model| {
            verify_password_pbkdf2_sha256(
                &request.password,
                &user_model.id.to_string(),
                &user_model.password,
            )
        }) else {
            return Err(AppError::bad_request("用户名或密码错误"));
        };

        self.get_token_by_user_id(user_model.id, None).await
    }

    /// 手机号短信验证码登录并签发 Token
    pub async fn get_token_by_sms(
        &self,
        request: GetTokenBySmsRequest,
    ) -> Result<String, AppError> {
        Self::validate_get_token_by_sms_request(&request)?;

        // 验证码在有效期内保留，按原始输入匹配手机号和验证码
        self.verify_phone_code(&request.phone, &request.verify_code)
            .await?;

        let existing_user = user::Entity::find()
            .filter(user::Column::Phone.eq(&request.phone))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户查询失败: {error}")))?;

        let user_id = if let Some(existing_user) = existing_user {
            existing_user.id
        } else {
            // 短信首登时自动补齐一条最小可用用户记录
            let user_id = self
                .context
                .app
                .id_generator
                .get_id()
                .await
                .map_err(|error| {
                    AppError::internal(format!("生成短信登录用户主键失败: {error}"))
                })?;
            let audit_fields = build_system_create_audit_fields();
            let user_name = uuid::Uuid::new_v4().to_string();
            let display_name = format!("{}手机短信新用户", Utc::now().format("%m/%d/%Y %H:%M:%S"));
            let password = hash_password_pbkdf2_sha256(
                &uuid::Uuid::new_v4().to_string(),
                &user_id.to_string(),
            );

            user::ActiveModel {
                row_version: Default::default(),
                id: Set(user_id),
                user_name: Set(user_name),
                name: Set(display_name),
                phone: Set(request.phone),
                email: Set(None),
                password: Set(password),
                create_time: Set(audit_fields.create_time),
                delete_time: Set(audit_fields.delete_time),
                update_time: Set(None),
                create_user_id: Set(None),
                update_user_id: Set(None),
                delete_user_id: Set(None),
            }
            .insert_versioned(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("短信登录用户创建失败: {error}")))?;

            user_id
        };

        self.get_token_by_user_id(user_id, None).await
    }

    /// 微信小程序登录并签发 Token。
    pub async fn get_token_by_wx_mini_app(
        &self,
        request: GetTokenByWxMiniAppRequest,
    ) -> Result<String, AppError> {
        let open_id = self
            .get_wx_mini_app_open_id(&request.app_id, &request.code)
            .await?;
        let existing = self
            .find_external_user("WeiXinMiniApp", &request.app_id, Some(&open_id))
            .await?;
        let user_id = if let Some(user_id) = existing {
            user_id
        } else {
            let guard = self
                .context
                .authorize_lock
                .lock(
                    &format!("GetTokenByWeiXinMiniAppCode{open_id}"),
                    std::time::Duration::ZERO,
                    1,
                )
                .await?;
            // 获锁后再次读取；正常与错误结果都先等待异步解锁。
            let result = async {
                if let Some(user_id) = self
                    .find_external_user("WeiXinMiniApp", &request.app_id, Some(&open_id))
                    .await?
                {
                    Ok(user_id)
                } else {
                    let name =
                        format!("{}微信小程序新用户", Utc::now().format("%m/%d/%Y %H:%M:%S"));
                    self.create_external_user("WeiXinMiniApp", &request.app_id, &open_id, name)
                        .await
                }
            }
            .await;
            guard.release().await;
            result?
        };
        self.get_token_by_user_id(user_id, None).await
    }

    /// 微信公众号网页登录并签发 Token。
    pub async fn get_token_by_wx_app(
        &self,
        request: GetTokenByWxAppRequest,
    ) -> Result<String, AppError> {
        let (access_token, open_id) = self
            .get_wx_app_access_token_and_open_id(&request.app_id, &request.code)
            .await?;
        let user_info = self.get_wx_app_user_info(&access_token, &open_id).await?;
        let name = user_info
            .nick_name
            .ok_or_else(|| AppError::bad_request("微信授权失败"))?;
        // 空昵称仍是有效返回；查询使用 userinfo 的 OpenId，新增绑定使用换取令牌时的 OpenId。
        let existing = self
            .find_external_user("WeiXinApp", &request.app_id, user_info.open_id.as_deref())
            .await?;
        let user_id = match existing {
            Some(user_id) => user_id,
            None => {
                self.create_external_user("WeiXinApp", &request.app_id, &open_id, name)
                    .await?
            }
        };
        self.get_token_by_user_id(user_id, None).await
    }

    /// 绑定和关联用户都必须仍有效，不能为软删除账户重新签发令牌。
    async fn find_external_user(
        &self,
        app_name: &str,
        app_id: &str,
        open_id: Option<&str>,
    ) -> Result<Option<i64>, AppError> {
        let query = user_bind_external::Entity::find()
            .inner_join(user::Entity)
            .filter(user_bind_external::Column::AppName.eq(app_name))
            .filter(user_bind_external::Column::AppId.eq(app_id))
            .filter(user_bind_external::Column::DeleteTime.is_null())
            .filter(user::Column::DeleteTime.is_null());
        let query = match open_id {
            Some(open_id) => query.filter(user_bind_external::Column::OpenId.eq(open_id)),
            None => query.filter(user_bind_external::Column::OpenId.is_null()),
        };
        Ok(query
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信绑定记录查询失败: {error}")))?
            .map(|bind| bind.user_id))
    }

    /// 首登用户和外部绑定作为一次保存提交，不留下半注册账户。
    async fn create_external_user(
        &self,
        app_name: &str,
        app_id: &str,
        open_id: &str,
        name: String,
    ) -> Result<i64, AppError> {
        let user_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成微信用户主键失败: {error}")))?;
        let bind_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成微信绑定主键失败: {error}")))?;
        let audit = build_system_create_audit_fields();
        let password =
            hash_password_pbkdf2_sha256(&uuid::Uuid::new_v4().to_string(), &user_id.to_string());
        let tx = crate::write_support::begin_transaction(self.context.app.db).await?;
        user::ActiveModel {
            row_version: Default::default(),
            id: Set(user_id),
            user_name: Set(uuid::Uuid::new_v4().to_string()),
            name: Set(name),
            phone: Set(String::new()),
            email: Set(None),
            password: Set(password),
            create_time: Set(audit.create_time),
            delete_time: Set(None),
            update_time: Set(None),
            create_user_id: Set(None),
            update_user_id: Set(None),
            delete_user_id: Set(None),
        }
        .insert_versioned(&tx)
        .await
        .map_err(|error| AppError::internal(format!("微信用户创建失败: {error}")))?;
        user_bind_external::ActiveModel {
            row_version: Default::default(),
            id: Set(bind_id),
            user_id: Set(user_id),
            app_name: Set(app_name.to_string()),
            app_id: Set(app_id.to_string()),
            open_id: Set(open_id.to_string()),
            create_time: Set(audit.create_time),
            delete_time: Set(None),
        }
        .insert_versioned(&tx)
        .await
        .map_err(|error| AppError::internal(format!("微信绑定创建失败: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交微信注册失败: {error}")))?;
        Ok(user_id)
    }

    /// 按 AppId 所属的分组读取密钥；空分组使用 SQL IS NULL。
    async fn get_wx_app_secret(
        &self,
        module: &str,
        app_id: &str,
    ) -> Result<Option<String>, AppError> {
        let group_id = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(module))
            .filter(app_setting::Column::Key.eq("AppId"))
            .filter(app_setting::Column::Value.eq(app_id))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信配置查询失败: {error}")))?
            .and_then(|setting| setting.group_id);
        let query = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(module))
            .filter(app_setting::Column::Key.eq("AppSecret"))
            .not_deleted(app_setting::Column::DeleteTime);
        let query = match group_id {
            Some(group_id) => query.filter(app_setting::Column::GroupId.eq(group_id)),
            None => query.filter(app_setting::Column::GroupId.is_null()),
        };
        Ok(query
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信 AppSecret 查询失败: {error}")))?
            .map(|setting| setting.value))
    }

    /// 微信授权码仅发送一次；失败信息不包含 URL 中的密钥、授权码或令牌。
    async fn get_wx_response(
        &self,
        url: &str,
        error_message: &'static str,
    ) -> Result<String, AppError> {
        let response = self
            .context
            .http_client_factory
            .client()
            .get(url)
            .send()
            .await
            .map_err(|_| AppError::internal(error_message))?;
        if !response.status().is_success() {
            return Err(AppError::internal(error_message));
        }
        response
            .text()
            .await
            .map_err(|_| AppError::internal(error_message))
    }

    /// 通过 AppId + Code 获取微信小程序 openId，并验证 session_key 存在。
    async fn get_wx_mini_app_open_id(&self, app_id: &str, code: &str) -> Result<String, AppError> {
        const ERROR: &str = "GetWeiXinMiniAppOpenIdAndSessionKey 获取失败";
        let secret = self
            .get_wx_app_secret("WeiXinMiniApp", app_id)
            .await?
            .ok_or_else(|| AppError::internal(ERROR))?;
        let url = format!(
            "https://api.weixin.qq.com/sns/jscode2session?appid={app_id}&secret={secret}&js_code={code}&grant_type=authorization_code"
        );
        let body = self.get_wx_response(&url, ERROR).await?;
        let open_id = common::json_helper::get_value_by_key(&body, "openid");
        let session_key = common::json_helper::get_value_by_key(&body, "session_key");
        match (open_id, session_key) {
            (Some(open_id), Some(_)) => Ok(open_id),
            _ => Err(AppError::internal(ERROR)),
        }
    }

    /// 通过 AppId + Code 获取微信公众号 AccessToken 与 OpenId。
    async fn get_wx_app_access_token_and_open_id(
        &self,
        app_id: &str,
        code: &str,
    ) -> Result<(String, String), AppError> {
        const ERROR: &str = "获取 AccessToken 失败";
        let secret = self
            .get_wx_app_secret("WeiXinApp", app_id)
            .await?
            .ok_or_else(|| AppError::internal(ERROR))?;
        let url = format!(
            "https://api.weixin.qq.com/sns/oauth2/access_token?appid={app_id}&secret={secret}&code={code}&grant_type=authorization_code"
        );
        let body = self.get_wx_response(&url, ERROR).await?;
        match (
            common::json_helper::get_value_by_key(&body, "access_token"),
            common::json_helper::get_value_by_key(&body, "openid"),
        ) {
            (Some(access_token), Some(open_id)) => Ok((access_token, open_id)),
            _ => Err(AppError::internal(ERROR)),
        }
    }

    /// 获取微信公众号用户信息；授权失败由调用方按昵称是否为 null 判断。
    async fn get_wx_app_user_info(
        &self,
        access_token: &str,
        open_id: &str,
    ) -> Result<GetWeiXinAppUserInfoDto, AppError> {
        let url = format!(
            "https://api.weixin.qq.com/sns/userinfo?access_token={access_token}&openid={open_id}"
        );
        let body = self.get_wx_response(&url, "获取微信用户信息失败").await?;
        common::json_helper::json_to_object(&body)
            .map_err(|_| AppError::internal("微信用户信息解析失败"))
    }

    /// 发送短信验证码
    pub async fn send_sms_verify_code(
        &self,
        request: SendSmsVerifyCodeRequest,
    ) -> Result<bool, AppError> {
        Self::validate_send_sms_verify_code_request(&request)?;

        let cache = self
            .context
            .cache
            .ok_or_else(|| AppError::internal("当前宿主未启用短信验证码缓存"))?;
        let code_key = build_phone_verify_code_key(&request.phone);
        // 源 IsContainKeyAsync 将缓存读取失败视为不存在。
        if matches!(cache.get_string(&code_key).await, Ok(Some(_))) {
            return Err(AppError::bad_request("请勿频繁获取验证码！"));
        }
        let code = rand::random_range(100000..999999u32).to_string();
        let mut template_params = IndexMap::new();
        template_params.insert("code".to_string(), Some(code.clone()));
        let payload = SendSmsDto {
            phone: Some(request.phone),
            sign_name: Some(String::new()),
            template_code: Some(String::new()),
            template_params: Some(template_params),
        }
        .to_queue_json()
        .map_err(|error| AppError::internal(format!("短信任务参数序列化失败: {error}")))?;
        let task_center_service = TaskCenterService::new(self.context.app);
        let task = task_center_service.create_queue_task_single(CreateQueueTaskDto {
            name: "MessageTask.SendSMS".to_string(),
            parameter: Some(payload),
            plan_time: None,
            callback_name: None,
            callback_parameter: None,
            parent_task_id: None,
        });
        let cache_write = cache.set_string_with_options(
            &code_key,
            &code,
            CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
        );
        // 与 Task.WhenAll 一致，等待入队和缓存都结束；源缓存扩展返回的失败标记不影响入队结果。
        let (task_result, _cache_result) = tokio::join!(task, cache_write);
        task_result?;

        Ok(true)
    }

    /// 通过短信验证码修改账户密码
    pub async fn update_password_by_sms(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordBySmsRequest,
    ) -> Result<bool, AppError> {
        Self::validate_update_password_by_sms_request(&request)?;
        let phone_user = user::Entity::find_by_id(current_user.user_id)
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户手机号查询失败: {error}")))?
            .ok_or_else(|| AppError::internal("用户手机号查询没有返回记录"))?;
        self.verify_phone_code(&phone_user.phone, &request.sms_code)
            .await?;
        // 验证码匹配后重新查询，对应源服务的第二次用户查询。
        let user_model = user::Entity::find_by_id(current_user.user_id)
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户信息查询失败: {error}")))?
            .ok_or_else(|| AppError::bad_request("账户不存在"))?;
        let tx = crate::write_support::begin_transaction(self.context.app.db).await?;
        let tokens = user_token::Entity::find()
            .filter(user_token::Column::UserId.eq(current_user.user_id))
            .not_deleted(user_token::Column::DeleteTime)
            .all(&tx)
            .await
            .map_err(|error| AppError::internal(format!("查询用户 Token 记录失败: {error}")))?;
        save_user_password(&tx, user_model, current_user.user_id, &request.new_password).await?;
        for token in tokens {
            token
                .into_active_model()
                .delete_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("清理用户 Token 记录失败: {error}")))?;
        }
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交密码变更失败: {error}")))?;
        Ok(true)
    }

    /// 通过旧密码修改账户密码
    pub async fn update_password_by_old_password(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordByOldPasswordRequest,
    ) -> Result<bool, AppError> {
        Self::validate_update_password_by_old_password_request(&request)?;
        let user_model = user::Entity::find_by_id(current_user.user_id)
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户信息查询失败: {error}")))?
            .ok_or_else(|| AppError::bad_request("账户异常，请联系后台工作人员"))?;
        if !verify_password_pbkdf2_sha256(
            &request.old_password,
            &current_user.user_id.to_string(),
            &user_model.password,
        ) {
            return Err(AppError::bad_request("原始密码验证失败"));
        }
        save_user_password(
            self.context.app.db,
            user_model,
            current_user.user_id,
            &request.new_password,
        )
        .await?;
        Ok(true)
    }

    /// 创建用户 Token 记录
    async fn create_user_token_record(
        &self,
        user_id: i64,
        last_token_id: Option<i64>,
    ) -> Result<i64, AppError> {
        let token_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成用户 Token 主键失败: {error}")))?;
        let audit_fields = build_system_create_audit_fields();

        let user_token_active_model = user_token::ActiveModel {
            row_version: Default::default(),
            id: Set(token_id),
            user_id: Set(user_id),
            last_id: Set(last_token_id),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
        };

        user_token_active_model
            .insert_versioned(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("Token 记录保存失败: {error}")))?;

        Ok(token_id)
    }

    /// 生成 JWT Token 字符串
    fn build_jwt_token(&self, user_id: i64, token_id: i64) -> Result<String, AppError> {
        let now = Utc::now();
        let claims = JwtClaims {
            token_id: token_id.to_string(),
            user_id: user_id.to_string(),
            iss: self.context.issuer.to_string(),
            aud: self.context.audience.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(self.context.expiry_seconds)).timestamp(),
            nbf: now.timestamp(),
        };

        self.context.jwt_codec.sign(&claims)
    }

    /// 根据用户 ID 生成 JWT Token
    pub async fn get_token_by_user_id(
        &self,
        user_id: i64,
        last_token_id: Option<i64>,
    ) -> Result<String, AppError> {
        // 先落库 Token 链记录 再返回签名后的 JWT 字符串
        let token_id = self
            .create_user_token_record(user_id, last_token_id)
            .await?;
        self.build_jwt_token(user_id, token_id)
    }

    /// 有效期过半时续签；同一旧令牌十分钟内复用新令牌，非阻塞锁避免并发签发。
    pub async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError> {
        let half_life_millis = (i128::from(nbf) + i128::from(exp)) * 500;
        if i128::from(Utc::now().timestamp_millis()) <= half_life_millis {
            return Ok(None);
        }
        let _renew_guard = match self
            .context
            .authorize_lock
            .try_lock(
                &format!("IssueNewToken{token_id}"),
                std::time::Duration::ZERO,
                1,
            )
            .await?
        {
            Some(guard) => guard,
            None => return Ok(None),
        };
        let cache = self
            .context
            .cache
            .ok_or_else(|| AppError::internal("当前宿主未启用续签缓存"))?;
        let cache_key = format!("{token_id}newToken");
        if let Some(token) = cache.get_string(&cache_key).await? {
            return Ok(Some(token));
        }
        let token = self.get_token_by_user_id(user_id, Some(token_id)).await?;
        let cleanup_guard = self
            .context
            .authorize_lock
            .try_lock("ClearExpireToken", std::time::Duration::ZERO, 1)
            .await?;
        if cleanup_guard.is_some() {
            let clear_time: DateTimeWithTimeZone = (Utc::now() - Duration::days(7)).into();
            user_token::Entity::delete_many()
                .filter(user_token::Column::CreateTime.lt(clear_time))
                .filter(user_token::Column::DeleteTime.is_null())
                .exec(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("清理过期 Token 记录失败: {error}")))?;
        }
        cache
            .set_string_with_options(
                &cache_key,
                &token,
                CacheEntryOptions::relative(std::time::Duration::from_secs(600)),
            )
            .await?;
        Ok(Some(token))
    }

    /// 检测当前用户是否具备指定路由权限
    pub async fn check_function_authorize(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        _allow_unbound: bool,
    ) -> Result<bool, AppError> {
        // 与源服务一致，授权判断读取当前数据库状态，不缓存授权结果。
        let route_record = function_route::Entity::find()
            .filter(function_route::Column::Module.eq(module))
            .filter(function_route::Column::Route.eq(route))
            .not_deleted(function_route::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限信息查询失败: {error}")))?;

        let result = match route_record {
            // 路由存在且已绑定功能 需要校验权限
            Some(route) if route.function_id.is_some() => {
                let function_id = route.function_id.unwrap();

                // 已绑定功能时 同时检查用户直授和角色继承的权限
                let role_ids = user_role::Entity::find()
                    .filter(user_role::Column::UserId.eq(current_user.user_id))
                    .not_deleted(user_role::Column::DeleteTime)
                    .all(self.context.app.db)
                    .await
                    .map_err(|error| AppError::internal(format!("用户角色查询失败: {error}")))?
                    .into_iter()
                    .map(|item| item.role_id)
                    .collect::<Vec<_>>();

                let mut authorize_condition = Condition::any()
                    .add(function_authorize::Column::UserId.eq(current_user.user_id));

                if !role_ids.is_empty() {
                    authorize_condition =
                        authorize_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
                }

                function_authorize::Entity::find()
                    .filter(function_authorize::Column::FunctionId.eq(function_id))
                    .not_deleted(function_authorize::Column::DeleteTime)
                    .filter(authorize_condition)
                    .one(self.context.app.db)
                    .await
                    .map_err(|error| AppError::internal(format!("权限数据查询失败: {error}")))?
                    .is_some()
            }
            // 路由存在但未绑定功能 视为公开接口 直接放行 故意这样设计的
            Some(_) => true,
            // 路由不存在 视为公开接口 直接放行 故意这样设计的
            None => true,
        };

        Ok(result)
    }

    /// 获取当前用户可访问的功能列表
    pub async fn get_function_list(
        &self,
        current_user: CurrentUser,
        sign: Option<&str>,
    ) -> Result<HashMap<String, String>, AppError> {
        let role_ids = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(current_user.user_id))
            .not_deleted(user_role::Column::DeleteTime)
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户角色查询失败: {error}")))?
            .into_iter()
            .map(|item| item.role_id)
            .collect::<Vec<_>>();

        let mut authorize_condition =
            Condition::any().add(function_authorize::Column::UserId.eq(current_user.user_id));

        if !role_ids.is_empty() {
            authorize_condition =
                authorize_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
        }

        let mut query = function_authorize::Entity::find()
            .find_also_related_active(repository::function::Entity)
            .not_deleted(function_authorize::Column::DeleteTime)
            .filter(repository::function::Column::DeleteTime.is_null())
            .filter(authorize_condition);

        if let Some(sign) = sign {
            query = query.filter(repository::function::Column::Sign.eq(sign));
        }

        // 同一功能可能通过多条授权命中 这里用 Map 去重
        let mut result = HashMap::new();

        for (_, function) in query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("功能权限列表查询失败: {error}")))?
        {
            let Some(function) = function else {
                continue;
            };

            result.entry(function.sign).or_insert(function.name);
        }

        Ok(result)
    }

    /// 获取路由权限绑定列表
    pub async fn get_function_route_list(
        &self,
        request: GetFunctionRouteListRequest,
    ) -> Result<Vec<FunctionRouteDto>, AppError> {
        // 先做数据库粗筛 再在内存中补充关键词匹配
        let mut query = function_route::Entity::find()
            .find_also_related_active(function::Entity)
            .not_deleted(function_route::Column::DeleteTime);

        if let Some(module) = request
            .module
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query = query.filter(function_route::Column::Module.eq(module));
        }

        let keyword = request
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned);

        let route_list = query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限绑定列表查询失败: {error}")))?;

        Ok(route_list
            .into_iter()
            .filter(|(route, function)| match keyword.as_deref() {
                Some(keyword) => {
                    route.route.contains(keyword)
                        || route
                            .remark
                            .as_deref()
                            .is_some_and(|item| item.contains(keyword))
                        || function
                            .as_ref()
                            .is_some_and(|item| item.name.contains(keyword))
                        || function
                            .as_ref()
                            .is_some_and(|item| item.sign.contains(keyword))
                }
                None => true,
            })
            .map(|(route, function)| FunctionRouteDto {
                id: route.id.to_string(),
                module: route.module,
                route: route.route,
                remark: route.remark,
                function_id: function.as_ref().map(|item| item.id.to_string()),
                function_name: function.as_ref().map(|item| item.name.clone()),
                function_sign: function.as_ref().map(|item| item.sign.clone()),
                create_time: route.create_time.to_rfc3339(),
            })
            .collect())
    }

    /// 设置路由权限绑定
    pub async fn set_function_route(&self, request: SetFunctionRouteDto) -> Result<bool, AppError> {
        // 仅允许把路由绑定到叶子功能节点 避免绑定模块节点导致语义混乱
        if request.route_id <= 0 {
            return Err(AppError::bad_request("无效的 routeId"));
        }

        if let Some(function_id) = request.function_id {
            if function_id <= 0 {
                return Err(AppError::bad_request("无效的 functionId"));
            }

            let function_model = function::Entity::find_by_id(function_id)
                .not_deleted(function::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("功能信息查询失败: {error}")))?;

            let Some(function_model) = function_model else {
                return Err(AppError::bad_request("指定的功能不存在"));
            };

            if function_model.r#type != FUNCTION_TYPE_FUNCTION {
                return Err(AppError::bad_request("路由仅允许绑定功能节点"));
            }
        }

        let route_model = function_route::Entity::find_by_id(request.route_id)
            .not_deleted(function_route::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限信息查询失败: {error}")))?;

        let Some(route_model) = route_model else {
            return Err(AppError::bad_request("指定的路由不存在"));
        };

        let before_log = json!({
            "id": route_model.id,
            "module": route_model.module,
            "route": route_model.route,
            "remark": route_model.remark,
            "function_id": route_model.function_id,
        });

        let mut active = route_model.into_active_model();
        active.function_id = Set(request.function_id);
        let updated_model = active
            .update_versioned(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限绑定更新失败: {error}")))?;

        // 记录路由权限绑定关系变更
        write_data_update_log(
            self.context.app,
            "FunctionRoute",
            updated_model.id,
            "set_function_route",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "module": updated_model.module,
                "route": updated_model.route,
                "remark": updated_model.remark,
                "function_id": updated_model.function_id,
            })),
            None,
        )
        .await?;

        Ok(true)
    }

    /// 获取功能树
    pub async fn get_function_tree(&self) -> Result<Vec<FunctionTreeDto>, AppError> {
        load_function_tree(self.context.app.db, None, None)
            .await
            .map_err(|error| AppError::internal(error.to_string()))
    }

    /// 校验手机号验证码
    async fn verify_phone_code(&self, phone: &str, verify_code: &str) -> Result<(), AppError> {
        let Some(cache) = self.context.cache else {
            return Err(AppError::internal("当前宿主未启用缓存，无法校验短信验证码"));
        };

        let code_key = build_phone_verify_code_key(phone);
        let cached_code = cache.get_string(&code_key).await?;

        match cached_code.as_deref() {
            Some(code) if !code.is_empty() && code == verify_code => Ok(()),
            _ => Err(AppError::bad_request("短信验证码错误")),
        }
    }
}

/// 只在密码或修改者变化时保存，沿用源仓储的变更跟踪语义。
async fn save_user_password<C: ConnectionTrait + TransactionTrait>(
    db: &C,
    user_model: user::Model,
    actor_user_id: i64,
    password: &str,
) -> Result<(), AppError> {
    let password = hash_password_pbkdf2_sha256(password, &user_model.id.to_string());
    if user_model.password == password && user_model.update_user_id == Some(actor_user_id) {
        return Ok(());
    }
    let mut active = user_model.into_active_model();
    active.password = Set(password);
    active.update_user_id = Set(Some(actor_user_id));
    active
        .update_versioned(db)
        .await
        .map_err(|error| AppError::internal(format!("密码更新失败: {error}")))?;
    Ok(())
}

/// 构建手机号验证码缓存键
fn build_phone_verify_code_key(phone: &str) -> String {
    format!("VerifyPhone_{phone}")
}

/// 递归加载功能树
#[async_recursion]
async fn load_function_tree(
    db: &DatabaseConnection,
    parent_id: Option<i64>,
    parent_name: Option<String>,
) -> Result<Vec<FunctionTreeDto>, DbErr> {
    // 先取当前层模块 再批量加载其直接功能节点和递归子模块
    let module_query = match parent_id {
        Some(parent_id) => {
            function::Entity::find().filter(function::Column::ParentId.eq(parent_id))
        }
        None => function::Entity::find().filter(function::Column::ParentId.is_null()),
    };

    let module_list = module_query
        .filter(function::Column::Type.eq(FUNCTION_TYPE_MODULE))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    if module_list.is_empty() {
        return Ok(vec![]);
    }

    let module_ids = module_list.iter().map(|item| item.id).collect::<Vec<_>>();

    let child_functions = function::Entity::find()
        .filter(function::Column::ParentId.is_in(module_ids.clone()))
        .filter(function::Column::Type.eq(FUNCTION_TYPE_FUNCTION))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    let mut result = Vec::with_capacity(module_list.len());

    for module in module_list {
        // 子功能名称会裁掉父模块前缀 让前端展示更简洁
        let children = load_function_tree(db, Some(module.id), Some(module.name.clone())).await?;
        let functions = child_functions
            .iter()
            .filter(|item| item.parent_id == Some(module.id))
            .map(|item| FunctionTreeDto {
                id: item.id.to_string(),
                name: trim_prefix_name(&item.name, Some(&module.name)),
                sign: item.sign.clone(),
                remark: item.remark.clone(),
                child_list: vec![],
                function_list: vec![],
            })
            .collect::<Vec<_>>();

        result.push(FunctionTreeDto {
            id: module.id.to_string(),
            name: trim_prefix_name(&module.name, parent_name.as_deref()),
            sign: module.sign,
            remark: module.remark,
            child_list: children,
            function_list: functions,
        });
    }

    Ok(result)
}
