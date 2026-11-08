repository_core::migration::Migration {
    version: 20261107105933,
    name: "align_stored_file_table",
    build_statements: |_| repository_core::migration::model::statements(include_str!("up.json")),
    build_down_statements: |_| repository_core::migration::model::statements(include_str!("down.json")),
    model_snapshot: Some(include_str!("model.json")),
}
