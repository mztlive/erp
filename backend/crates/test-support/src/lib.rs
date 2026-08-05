//! P0-3 测试夹具：`TestDb`、`require_mongo!`、种子与断言辅助。
//!
//! 只作为 dev-dependency 使用；所有需要真实 MongoDB 的测试统一
//! `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（见 conventions 7.2）。
