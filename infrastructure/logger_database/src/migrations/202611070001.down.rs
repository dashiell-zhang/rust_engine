// 只撤销所属模型；分区父表的已附加子分区由 PostgreSQL 一并删除。
&[
    r#"DROP TABLE "Log""#,
]
