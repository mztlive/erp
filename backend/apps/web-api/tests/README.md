# web-api 集成测试（P6 收口）

本目录在 P3 实现阶段**故意留空**，避免与实现漂移。

按 `docs/dev-plan/P6-integration-tests.md` 在 **I-G\*** / **I-X\*** 子阶段补齐：

- 域内：`<domain>_api.rs`
- 跨域：`invariants/**`、`concurrency/**`
- 门控：`#[ignore]` + `ERP_TEST_MONGO_URI`（`test-support`）
- 覆盖表见 P6 §1.2–§1.5

勿在 P3 PR 中提交业务集成测试。
