//! 当前模型的命名索引声明；模型变化在此调整，再生成新迁移，不修改历史产物。
pub fn indexes() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "User",
            "IX_User_UserName",
            r#"CREATE INDEX "IX_User_UserName" ON "User" ("UserName")"#,
        ),
        (
            "User",
            "IX_User_Phone",
            r#"CREATE INDEX "IX_User_Phone" ON "User" ("Phone")"#,
        ),
        (
            "User",
            "IX_User_Email",
            r#"CREATE INDEX "IX_User_Email" ON "User" ("Email")"#,
        ),
        (
            "User",
            "IX_User_CreateTime",
            r#"CREATE INDEX "IX_User_CreateTime" ON "User" ("CreateTime")"#,
        ),
        (
            "User",
            "IX_User_DeleteTime",
            r#"CREATE INDEX "IX_User_DeleteTime" ON "User" ("DeleteTime")"#,
        ),
        (
            "User",
            "IX_User_UpdateTime",
            r#"CREATE INDEX "IX_User_UpdateTime" ON "User" ("UpdateTime")"#,
        ),
        (
            "User",
            "IX_User_CreateUserId",
            r#"CREATE INDEX "IX_User_CreateUserId" ON "User" ("CreateUserId")"#,
        ),
        (
            "User",
            "IX_User_DeleteUserId",
            r#"CREATE INDEX "IX_User_DeleteUserId" ON "User" ("DeleteUserId")"#,
        ),
        (
            "User",
            "IX_User_UpdateUserId",
            r#"CREATE INDEX "IX_User_UpdateUserId" ON "User" ("UpdateUserId")"#,
        ),
        (
            "UserBindExternal",
            "IX_UserBindExternal_UserId",
            r#"CREATE INDEX "IX_UserBindExternal_UserId" ON "UserBindExternal" ("UserId")"#,
        ),
        (
            "UserBindExternal",
            "IX_UserBindExternal_CreateTime",
            r#"CREATE INDEX "IX_UserBindExternal_CreateTime" ON "UserBindExternal" ("CreateTime")"#,
        ),
        (
            "UserBindExternal",
            "IX_UserBindExternal_DeleteTime",
            r#"CREATE INDEX "IX_UserBindExternal_DeleteTime" ON "UserBindExternal" ("DeleteTime")"#,
        ),
        (
            "Function",
            "IX_Function_Sign",
            r#"CREATE INDEX "IX_Function_Sign" ON "Function" ("Sign")"#,
        ),
        (
            "FunctionAuthorize",
            "IX_FunctionAuthorize_FunctionId",
            r#"CREATE INDEX "IX_FunctionAuthorize_FunctionId" ON "FunctionAuthorize" ("FunctionId")"#,
        ),
        (
            "FunctionAuthorize",
            "IX_FunctionAuthorize_RoleId",
            r#"CREATE INDEX "IX_FunctionAuthorize_RoleId" ON "FunctionAuthorize" ("RoleId")"#,
        ),
        (
            "FunctionAuthorize",
            "IX_FunctionAuthorize_UserId",
            r#"CREATE INDEX "IX_FunctionAuthorize_UserId" ON "FunctionAuthorize" ("UserId")"#,
        ),
        (
            "FunctionRoute",
            "IX_FunctionRoute_FunctionId",
            r#"CREATE INDEX "IX_FunctionRoute_FunctionId" ON "FunctionRoute" ("FunctionId")"#,
        ),
        (
            "LlmApp",
            "IX_LlmApp_Code",
            r#"CREATE INDEX "IX_LlmApp_Code" ON "LlmApp" ("Code")"#,
        ),
        (
            "Product",
            "IX_Product_SKU",
            r#"CREATE INDEX "IX_Product_SKU" ON "Product" ("SKU")"#,
        ),
        (
            "Product",
            "IX_Product_Name",
            r#"CREATE INDEX "IX_Product_Name" ON "Product" ("Name")"#,
        ),
        (
            "Order",
            "IX_Order_OrderNo",
            r#"CREATE INDEX "IX_Order_OrderNo" ON "Order" ("OrderNo")"#,
        ),
        (
            "UserInfo",
            "IX_UserInfo_UserId",
            r#"CREATE UNIQUE INDEX "IX_UserInfo_UserId" ON "UserInfo" ("UserId")"#,
        ),
        (
            "DataUpdateLog",
            "IX_DataUpdateLog_Table",
            r#"CREATE INDEX "IX_DataUpdateLog_Table" ON "DataUpdateLog" ("Table")"#,
        ),
        (
            "DataUpdateLog",
            "IX_DataUpdateLog_TableId",
            r#"CREATE INDEX "IX_DataUpdateLog_TableId" ON "DataUpdateLog" ("TableId")"#,
        ),
    ]
}

/// 源业务实体显式声明的普通索引；不因应用服务查重改成唯一索引。
pub fn declared_indexes() -> &'static [(&'static str, &'static str)] {
    &[
        ("AppSetting", "Module"),
        ("Article", "Title"),
        ("Category", "Name"),
        ("Function", "Sign"),
        ("LlmApp", "Code"),
        ("LlmApp", "IsEnable"),
        ("LlmModel", "IsEnable"),
        ("Link", "Sort"),
        ("DataUpdateLog", "Table"),
        ("DataUpdateLog", "TableId"),
        ("Product", "SKU"),
        ("Product", "Name"),
        ("Order", "OrderNo"),
        ("RegionProvince", "Province"),
        ("RegionCity", "City"),
        ("RegionArea", "Area"),
        ("RegionTown", "Town"),
        ("StoredFile", "Table"),
        ("StoredFile", "TableId"),
        ("StoredFile", "Sign"),
        ("User", "UserName"),
        ("User", "Phone"),
        ("User", "Email"),
    ]
}
