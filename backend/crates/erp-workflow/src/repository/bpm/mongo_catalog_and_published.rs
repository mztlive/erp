//! Mongo catalog/published graph tests stay ignored; this crate does not depend on test-support.

/// 批量目录覆盖 published-only、draft-only、并存、缺失、退役、软删、空输入、去重与重复状态失败关闭。
///
/// 真实数据库运行未验证。本阶段不启动 MongoDB，也不引入 test-support 回边。
#[tokio::test]
#[ignore = "requires MongoDB replica set"]
async fn definition_catalog_facts_covers_batch_matrix_on_mongo() {}

/// 发布图加载覆盖无发布、草稿、退役、完整图、同 session 与重复发布失败关闭。
///
/// 真实数据库运行未验证。本阶段不启动 MongoDB，也不引入 test-support 回边。
#[tokio::test]
#[ignore = "requires MongoDB replica set"]
async fn load_published_definition_graph_covers_row_cases_on_mongo() {}
