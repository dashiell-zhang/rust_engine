// 已登记迁移的固定 SQL；后续模型变化必须新增迁移，不能改写此文件。
(
&[
    r#"CREATE TABLE "Log" ( "Id" bigint NOT NULL PRIMARY KEY, "Project" text NOT NULL, "MachineName" text NOT NULL, "Level" text NOT NULL, "Category" text NOT NULL, "Content" text NOT NULL, "CreateTime" timestamp with time zone NOT NULL, "DeleteTime" timestamp with time zone ) PARTITION BY RANGE ("Id")"#,
    r#"CREATE INDEX IF NOT EXISTS "IX_Log_Category_CreateTime_NotDeleted"
            ON "Log" ("Category", "CreateTime") WHERE "DeleteTime" IS NULL"#,
    r#"CREATE INDEX IF NOT EXISTS "IX_Log_Project_CreateTime_NotDeleted"
            ON "Log" ("Project", "CreateTime") WHERE "DeleteTime" IS NULL"#,
],
    repository_core::partition::PartitionTableDefinition {
        schema: None,
        table: "Log".into(),
        key_column: "Id".into(),
        interval: 1,
        unit: repository_core::partition::PartitionUnit::Month,
    },
)
