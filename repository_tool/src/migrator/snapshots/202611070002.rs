// 已登记迁移的固定 SQL；后续模型变化必须新增迁移，不能改写此文件。
&[
    r#"DROP INDEX IF EXISTS "IX_UserRole_UserId_RoleId_NotDeleted""#,
    r#"CREATE INDEX "IX_UserRole_UserId_RoleId_NotDeleted" ON "UserRole" ("UserId", "RoleId") WHERE "UserRole"."DeleteTime" IS NULL"#,
]
