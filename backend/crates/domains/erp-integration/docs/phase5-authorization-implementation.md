# FSY-11：第五阶段授权修复（R1 开发）

本轮已实现 INT-AUTH-01、INT-AUTH-03，并补充 INT-AUTH-02 中本域可完成的交付、拒绝和仓储守卫回归。**INT-AUTH-02 尚未全部完成**：完整正向 query/require 场景需要 FSY-9 的共享 S-01 测试设施；当前不提交“全部修复完成”或独立 Review 通过的结论。

## 准确版本与协作边界

- 本轮分支：`agent/agent/fsy-11-363c002c07f7`；起始提交：`b650abb5da962539753c82529e07cb652243d75a`。
- 已读取分析交付 `b8f78e6e73ad8fe1b7db5b51534b8a949b12a080`，将原分析报告以 `16e14c13e` 接续到本分支；本轮起点与分析基准 `5ca681584940c7b5a9a6847cad345929eb2b647f` 的 integration/identity 实现相同。
- 复用父事项已发布的 FSY-7 共享提交 `6e25bc4c532e9bc478e363531c46f1a2df539f69`，本分支接续提交为 `7a88664c9`。只增加 `test-support` feature 下的恒拒绝 CompiledAccess 测试凭证，没有另建共享夹具、放宽生产 Builder 或修改生产授权政策。
- 本轮 integration 实现及最终报告提交 SHA 记录在事项最终评论，不在文档中写自身 SHA。
- 已核对父事项、FSY-9/39 评论及同族运行；FSY-9 为 `in_progress`，正在处理 S-01，尚未发布相应实现 SHA。运行列表有 20 条截断上限，未将未列出的任务视为未运行。
- 未修改 server 授权装配、FSY-35 编排、FSY-38 详情读模型、FSY-36 凭据/协议或第六阶段的应用切换。

## 问题逐项处置

| 编号 | 实现与证据 | 当前结论 |
| --- | --- | --- |
| INT-AUTH-01 / P2 | `service/reconciliation_difference.rs` 将已过滤根页交给 `reconciliation_difference/delivery.rs`：读取当前页最新决定 → 使用原投影派生状态与决定序号 → 对原凭证、原 executor 调用 validate → 返回页和范围元数据。删掉原先补齐前的 validate，没有重复验证旧阶段。 | 代码修复完成；5 项实际生产交付步骤的内存回归通过，供后续独立 Review。 |
| INT-AUTH-02 / P2 | 两类列表直接调用实际私有 executor 步骤，验证缺 Mapping 与身份设施故障先于业务读取。两类真实 Mapping 模型经共享凭证进入实际仓储守卫，验证正确模型、逐字段错配、其他执行器、阶段结束、事务重启、原始会话暴露、过期。交付回归见下表。 | 部分补证完成；完整正向身份/范围查询、空授权范围与精确差异决定的消费者行为证据仍待 S-01，保持未关闭。 |
| INT-AUTH-03 / P3 | 删除零调用的 `IntegrationAccess::require_handler`，同时删除创建动作白名单及目标推断。全仓 Rust 调用检索无调用方；上层创建用例仍显式使用 Operation，差异决定仍使用真实根 ID/base.version 的 Task。 | 清理完成。删除的是公开 Rust helper，存在源码 API 变化；仓库内无需调用方适配。未运行禁用的 server/workspace 全量编译。 |

交付 helper 只隔离最新决定读取和最终 validate 两个异步步骤。生产实现分别调用原仓储与共享 `AuthorizationService::validate`；测试替身执行同一个 `deliver`，不复制编排、不模拟 Mongo 事务成功，也不通过源码文本断言测试行为。原来的行投影仅从异步读取中提取为同步函数，其字段映射保持一致。

## 本轮实际行为回归

均为本 crate 的库单元测试；以下路径相对 `src/`。新增 10 项测试，完整库测试从 181 项增至 191 项。

| 场景 | 实际测试及断言 | 证据限制 |
| --- | --- | --- |
| 最新决定先于交付验证；成功页保持原语义 | `service::reconciliation_difference::delivery::tests::latest_decisions_precede_validation_and_preserve_page_projection`；同一执行边界、仅当前页 ID、调用顺序、total/page/page_size、范围摘要、决定状态与序号、无决定时 None/0。根版本 87 不被误作决定版本 4。 | 实际交付编排及同步投影；替身不证明真实 query 允许或数据库读取。 |
| 空页仍最终验证 | `empty_page_still_requires_final_validation_and_preserves_total`；空 ID/空 items，仍 validate，保留 total。 | 不宣称执行 Mongo 空页。 |
| 最新决定读取失败 | `latest_decision_failure_stops_before_validation_and_delivery`；保留具体 Internal 错误，不调用 validate、不交付响应。 | 内存步骤故障。 |
| 最终验证失败 | `final_validation_failure_discards_the_enriched_page`；补齐后 Forbidden 原样返回，没有页结果。 | 检查错误传播与失败停止，不证明身份设施的实际撤权检测。 |
| 阶段推进及到期拒绝交付 | `ended_stage_and_expired_credentials_discard_the_enriched_page`；分别使用真实执行边界失效与真实 CompiledAccess::ensure_current 的到期错误，补齐后拒绝页。 | 过期凭证在测试中直接签发为过期，不声称复现墙钟恰好在 await 期间到期；最终生产 validate 的接线另经静态核对。 |
| 错误任务列表提前拒绝 | `service::error_task::authorization_tests::error_task_page_rejects_missing_mapping_and_identity_failure_before_business_reads`；缺绑定返回指定 ValidationError，身份设施故障返回指定 Internal。 | 使用无 Mongo 会话的 SnapshotExecutor；不执行真实数据库读写。 |
| 差异列表提前拒绝 | `service::reconciliation_difference::authorization_tests::difference_page_rejects_missing_mapping_and_identity_failure_before_business_reads`；同上，且进入历史操作人、根查询及决定读取前即停止。 | 不以这两个拒绝式设施证明有权限/无权限的功能资格分支。 |
| 正确模型及全部被核对字段的错配 | `repository::integration_ops::tests::repository_guard_accepts_each_current_object_model_and_rejects_every_mismatched_field`；两类真实 Mapping 的模型通过，resource/collection/key/id_field/output 分别错配被实际守卫拒绝。 | 恒拒绝凭证只验证消费边界，不签发允许行。 |
| 执行器、阶段、事务重启和原始会话 | `repository_guard_rejects_other_executors_and_every_ended_read_stage`；原凭证不能在其他 executor、next_read、restart 或 session 暴露后复用。 | next_read 模拟统一写入前推进的阶段；不是实际数据库写入/回滚测试。 |
| 过期仓储凭证 | `repository_guard_rejects_expired_access_with_the_existing_persistence_error`；两类模型均返回既有 EntityMetadataOutOfRange 错误。 | 没有改变既有错误映射，也不声称真实查询结果已验证。 |

## 验证结果

在 `backend/` 实际执行：

```bash
cargo fmt --all
env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked
cargo fmt --all -- --check
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-rust-size.sh --json
```

- 指定库测试退出 0：**191 passed、0 failed、0 ignored、0 filtered out**。依赖 `proc-macro-error2 v2.0.1` 有现存 future-incompatibility 提醒。
- 首次编译发现测试导入了私有 Mapping 类型和一个 ID 类型推断错误，均改为既有公开 mapping 工厂/明确 ID 入参；共享目标目录还复用了未包含新 fixture 的旧 identity 产物，更新本地源码 mtime 后按同一条库测试命令正确重编译。最终结果以上述 191 项为准；未修改或清理其他 worktree 的缓存。
- 格式检查、领域/审计边界检查退出 0。
- 全仓源码体积门禁退出 1：59 项既有方法超限，均不在本轮变更文件。另按原门禁的 `scan_source` 检查本轮全部 8 个变更 Rust 文件（含已复用共享 fixture）无命中；未添加例外或提高阈值。全仓门禁不能标为通过。
- `git diff --check` 通过；静态门禁生成的跟踪中 Python bytecode 已恢复，未提交临时产物。
- 没有运行集成测试、examples、真实 MongoDB/S3/外部服务测试、其他 crate 的库测试、server/workspace 编译或 clippy all-targets；lib 通过后没有重复 cargo check。

## INT-AUTH-02 的必要输入与恢复条件

已有 unavailable/unbound 设施仅固定报错；本轮复用的 compiled_access 只签发恒拒绝条件且不属于生产 AuthorizationService，不能拿它替代真实 query/require 的成功凭证。生产身份 Facilities 仍为私有，不能在本域构造可控功能资格、范围与真实任务事实，也不能为消除测试障碍放宽 Builder。

FSY-9 已在父事项登记 S-01 的统一归属且当前正在实现。等待它发布可读取的共享提交，支持内存身份资格/范围、真实公共 query/require/validate 以及可绑定本域 Mapping/Facts。若 S-01 不包含精确任务加载，还需要兼容的窄读取替身，以执行 `require_decide` 的真实根目标读取；不得把数据库故障当作精确授权拒绝。

恢复后在本分支继续新增提交，逐项补齐：两类列表允许及功能拒绝、空范围不降级全量、真实范围/事实缺失、精确差异 owner/版本成功与错人/错版本/缺事实拒绝。禁止复制身份算法或用通用审批任务的既有回归替代本域消费者证据。上述必要证据全部完成后，再交 `in_review` 由后端审查检查准确交付 SHA；目前保持 blocked，避免将证据缺失写成实现完成或反复切换审查状态。

无需数据迁移、索引变更、权限分配或部署。真实数据库原子性、跨进程并发、HTTP/worker 运行及第六阶段切换仍不在本轮验证范围。
