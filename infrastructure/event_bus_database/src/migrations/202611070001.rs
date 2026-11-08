// 已登记迁移的固定 SQL；后续模型变化必须新增迁移，不能改写此文件。
&[
    r#"CREATE TABLE "EventBusStream" (
            "Id" BIGINT PRIMARY KEY, "Scope" VARCHAR(200) NOT NULL,
            "Position" BIGINT NOT NULL, "CreateTime" TIMESTAMPTZ NOT NULL, "DeleteTime" TIMESTAMPTZ NULL
        )"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "IX_EventBusStream_Scope" ON "EventBusStream" ("Scope")"#,
    r#"CREATE TABLE "EventBusMessage" (
            "Id" BIGINT PRIMARY KEY, "Scope" VARCHAR(200) NOT NULL, "Position" BIGINT NOT NULL,
            "EventId" UUID NOT NULL, "EventName" VARCHAR(1024) NOT NULL, "Payload" TEXT NOT NULL,
            "CreateTime" TIMESTAMPTZ NOT NULL, "DeleteTime" TIMESTAMPTZ NULL
        )"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "IX_EventBusMessage_Scope_Position" ON "EventBusMessage" ("Scope", "Position")"#,
    r#"CREATE INDEX IF NOT EXISTS "IX_EventBusMessage_Scope_CreateTime" ON "EventBusMessage" ("Scope", "CreateTime")"#,
]
