//! 当前模型的命名索引声明；模型变化在此调整，再生成新迁移，不修改历史产物。
pub fn indexes() -> &'static [(&'static str, &'static str, &'static str)] {
    &[(
        "TaskSetting",
        "IX_TaskSetting_Category_Name",
        r#"CREATE UNIQUE INDEX "IX_TaskSetting_Category_Name"
ON "TaskSetting" ("Category", "Name")
WHERE "DeleteTime" IS NULL AND (
    "Category" = 'QueueTask' OR (
        "Category" = 'ScheduleTask' AND ("Parameter" IS NULL OR "Parameter" = '__args_default__')
    )
)"#,
    )]
}
