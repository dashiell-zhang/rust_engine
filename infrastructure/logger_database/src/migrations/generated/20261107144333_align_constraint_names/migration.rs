repository_core::migration::Migration {
    version: 20261107144333,
    name: "align_constraint_names",
    build_statements: |_| repository_core::migration::model::statements(include_str!("up.json")),
    build_down_statements: |_| repository_core::migration::model::statements(include_str!("down.json")),
    model_snapshot: Some(include_str!("model.json")),
}
