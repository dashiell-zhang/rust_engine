repository_core::migration::Migration {
    version: 20261107141851,
    name: "align_source_log_indexes",
    build_statements: |_| repository_core::migration::model::statements(include_str!("up.json")),
    build_down_statements: |_| repository_core::migration::model::statements(include_str!("down.json")),
    model_snapshot: Some(include_str!("model.json")),
}
