// 已登记迁移的固定 SQL；后续模型变化必须新增迁移，不能改写此文件。
&[
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "UX_TaskSetting_Category_Name_ActiveDefinition"
ON "TaskSetting" ("Category", "Name")
WHERE "DeleteTime" IS NULL AND (
    "Category" = 'QueueTask' OR (
        "Category" = 'ScheduleTask' AND ("Parameter" IS NULL OR "Parameter" = '__args_default__')
    )
)"#,
    r#"DO $migration$
DECLARE constraint_name text;
BEGIN
    FOR constraint_name IN
        SELECT c.conname
        FROM pg_constraint c
        JOIN pg_attribute a ON a.attrelid = c.conrelid
            AND c.conkey = ARRAY[a.attnum]::smallint[]
        WHERE c.conrelid = '"TaskSetting"'::regclass AND c.contype = 'f'
            AND a.attname IN ('CreateUserId', 'UpdateUserId', 'DeleteUserId')
    LOOP
        EXECUTE format('ALTER TABLE "TaskSetting" DROP CONSTRAINT %I', constraint_name);
    END LOOP;
END;
$migration$"#,
]
