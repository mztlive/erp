# database 集成测试（P6 收口）

本目录在 P2 实现阶段**故意留空**，避免与实现漂移。

按 `docs/dev-plan/P6-integration-tests.md` 在 **I-G\*** 子阶段按域补齐：

- 文件命名：`<domain>_repository.rs`
- 门控：`#[ignore]` + `ERP_TEST_MONGO_URI`（`test-support`）
- 覆盖表见 P6 §1.1

勿在 P2 PR 中提交业务集成测试。
