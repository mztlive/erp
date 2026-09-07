# 阶段 14：W29 task_decision 拆分实施合同与静态证据

## 1. 输入与验收边界

- 唯一实施树：`/private/tmp/erp-domain-crate-14-integration`；读取 HEAD：`cb159e5bf874e54254ea6fc447347627572d1a16`。
- before 源码：`453cd48082793b8f40e5afa37d5d225a747fd1b0`。旧六叶已逐文件从该提交重取并与 `/private/tmp/integration14-task-decision-input` 完全比对；当前 HEAD 是阶段 14 地基提交，不能替代上述业务输入。
- after：本报告末尾 SHA256 对应当前未提交工作树。旧六叶删除；本片只写领域与流程的 task_decision 所属叶和 `/tmp` 证据。
- 本报告登记静态实施证据，**不是 Cargo、运行时、真实 DB、事务回滚或真实 RBAC 执行证明**。本 worker 未运行 Cargo/MongoDB，未改历史 `tests/**`，未提交。统一编译、Clippy、库测试与边界门禁由 root 登记。
- 机器证据：`/private/tmp/integration14-task-decision-result.json`。测试定义合计 17：原 5、新 12、ignore 0；本 worker 执行数为 0。

## 2. 公共 API 与唯一拥有者

| 所有者 | 实际出口 | 强制合同 |
| --- | --- | --- |
| Process | `IntegrationResolutionProcess::apply_task_action(IntegrationTaskActionCommand, &AuditActor) -> services::Result<IntegrationTaskActionResult>` | 原方法签名、操作审计名、回执优先级不变。 |
| Process | `IntegrationResolutionProcess::complete_task(IntegrationTaskCompletionCommand, &AuditActor) -> services::Result<IntegrationTaskCompletionResult>` | 外层持有正式任务、权限、事务和审计。 |
| Process | `IntegrationResolutionProcess::decide_difference(&str, DirectReconciliationCommand, &AuditActor) -> services::Result<DirectReconciliationResult>` | 路径身份校验在 identity 前；Prepared 仍在原事务内。 |
| Domain | `service::task_decision::action::execute_task_action(db, authority, command, receipt_id, actor_id, executor) -> erp_integration::Result<ActionFact>` | 本域读取、主题状态、证据动作、record_attempt/CAS 或不可变决定追加。 |
| Domain | `service::task_decision::complete::complete_domain_item(db, authority, command, resolution_id, actor_id, executor) -> erp_integration::Result<TerminalFact>` | 本域策略和终态证据验证，错误任务解决或差异决定追加。 |
| Domain | `service::task_decision::direct::execute_direct_decision(db, authority, prepared, command, receipt_id, actor_id, executor) -> erp_integration::Result<DirectFact>` | 流程完成无任务检查后才调用；领域保持 difference→latest→版本→open→证据→append→create。 |
| Domain | `guard::load_error_task_for_association(db, id, executor) -> erp_integration::Result<IntegrationErrorTask>` | 只完成原关联读取与 NotFound；此处不拒绝终态。原动作阶段继续第二次 load 与 terminal guard。 |
| Domain | `action::audit_log_reference(&str)`、`action::next_allowed_actions(item_type, outcome)` | 分别复用唯一 EvidenceRecordRef grammar 与 next_actions_after_outcome。 |
| Domain | 私有 `append_resolution`、`DirectFact` | 唯一追加工厂在 task_decision/mod.rs；不得在 reconciliation_difference 复制第二份。 |

注册要求已在当前根观察到：领域 `pub mod task_decision;`，流程 `mod task_decision;`，流程 crate 根 `pub mod integration_resolution;`。这些共享根由 A/B/root 拥有，本片未编辑。领域只依赖自身 DTO/entity/repository/证据 Port 及基础 crate；无 `services/database/entities/erp_workflow/erp_audit/erp_identity` 生产引用。

## 3. 三个入口逐步执行合同

| 路径 | before 入口 | after 入口／真实 runner | 必须保留的顺序 |
| --- | --- | --- | --- |
| 非终态动作 | `services/src/integration_ops/task_decision/action.rs:54` | `crates/erp-processes/src/integration_resolution/task_decision/action.rs:108`；`execution::run_action` | validate→完整命令 identity→receipt replay；未命中才 shared_rbac_service→PreparedWorkItemTarget→根事务→load_bound→真实 workflow 权限→本域动作与写入→subject_version→record_activity(now)→WorkItem CAS→结果投影→receipt。 |
| 任务完成 | `services/src/integration_ops/task_decision/complete.rs:46` | `crates/erp-processes/src/integration_resolution/task_decision/complete.rs:97`；`execution::run_completion` | validate→identity→receipt replay；未命中才 RBAC→Prepared→事务内 as_non_terminal_action→load_bound→真实权限→本域终态与写入→subject_version→complete_by_domain_command(now)→WorkItem CAS→receipt→结果投影。 |
| 直接对账 | `services/src/integration_ops/task_decision/direct.rs:42` | `crates/erp-processes/src/integration_resolution/task_decision/direct.rs:77`；`execution::run_direct` | validate→路径身份→identity→receipt replay；事务内 PreparedDirectDecisionTarget→ensure_no_work_item→领域读取及决定→receipt→direct result。 |

- `load_bound_work_item` 固定 WorkItem find→任务/subject 版本→status Open→当前 owner→原无操作 ensure_actor_eligible→正式关联。`if false` W29 独立任务分支与无操作函数仍保留，不启用新约束。
- 正式权限 provider 是 `services::workflow_compose::work_item_service(db.clone(), rbac.clone()).ensure_domain_decision_access(actor, item, executor)`；动作与完成均在本域动作之前真实调用，不用 DTO allowed actions 替代。
- 错误任务关联阶段仍独立读取错误任务，仅按 ResultUnknown 映射 IntegrationResultUnknown，其余映射 BusinessException；随后本域动作阶段第二次读取再拒绝 terminal。差异关联不提前读取差异。
- 直接对账的 `find_unique_for_reconciliation_difference` 返回任意任务均拒绝，未筛选 Open；域 difference 查询未提前。PreparedDirectDecisionTarget 的解析保持在根事务里，未迁至回执之前。
- 所有仓储、真实 workflow 授权和证据 Port 使用调用方原 `&mut dyn Executor`；production adapter 不创建 NoTransaction。只有回执 replay 继续按原样每次 fresh `NoTransaction`，根 `run_audited` 继续唯一持有事务。

## 4. 本域分支、证据和写入合同

| 分支 | 实际生产符号 | 固定先后关系 |
| --- | --- | --- |
| 错误任务非终态 | `crates/erp-integration/src/service/task_decision/action.rs:56` | load_error_task（NotFound/terminal）→subject→error_action_fact→原 compact summary→record_attempt(now)→integration_error_tasks.update→读取更新后的 base.version。 |
| 差异非终态 | `crates/erp-integration/src/service/task_decision/action.rs:184` | difference→latest→subject→open→difference_action_fact→append_resolution→resolution_no 投影→resolution create。 |
| 错误任务完成 | `crates/erp-integration/src/service/task_decision/complete.rs:51` | load/terminal→subject→error policy→ensure_completion_policy→verify refs→verified_reference→原 completion_resolution→ResolutionType::from_verified_evidence→transition Resolved(now)→CAS→base.version。 |
| 差异完成 | `crates/erp-integration/src/service/task_decision/complete.rs:100` | difference→latest→subject→open→policy→verify refs→reference→DirectConclusion::ConfirmValidDifference→append→resolution_no→create。 |
| 无任务决定 | `crates/erp-integration/src/service/task_decision/direct.rs:21` | difference→latest→typed direct version→open→direct_decision_fact→append→create。非终态仍复用 difference_action_fact；终态 reason registry 先于证据，DirectConclusion::from 保留。 |

证据统一消费调用方注入的 `&dyn erp_integration::ports::evidence::IntegrationEvidenceAuthority`。Process 在原 Prepared 后 clone 同一 Arc，direct 在事务开始前仅 clone Arc；clone 不读取、授权、生成 ID 或时间。生产权威实现由 B 的唯一 `MongoIntegrationEvidenceAuthority` 提供，不对 Database 实现证据 Port。

- QueryOriginalResult：`query_original` 先执行；Terminal 才 `discover_evidence`；NoResult/Unknown 不 discover、不 replay。
- ReplayOriginal：错误任务先 `can_replay_original`，原错误文案不变；差异分支无此错误任务规则，直接调用 authority.replay_original。
- Reattribute：先 verify_reattribution，后 discover_evidence；任何前步错误停止。
- AddEvidence/LinkCompensation/完成/终态直接决定：使用同一 `service::evidence::verify_evidence_refs` 与 `verified_reference`。LinkCompensation 先检查 CompensationResult 类型存在；顺序没有提前或合并。
- verify_evidence_refs 的逐证据循环及新增测试由 B 拥有，本片只消费该唯一 helper；不得在本片复制证据 grammar、政策或跨域仓储规则。

## 5. 时间、ID、摘要与首错合同

| 事实 | before | after | 保持要求 |
| --- | --- | --- | --- |
| 非终态错误任务时钟 | action::execute_error_task_action 内 record_attempt | `crates/erp-integration/src/service/task_decision/action.rs:67` | evidence 和 summary 成功之后、CAS 之前调用一次。 |
| 完成错误任务时钟 | complete::complete_error_task 内 transition | `crates/erp-integration/src/service/task_decision/complete.rs:91` | 所有 policy、evidence、resolution_type 形成之后、CAS 之前。 |
| 差异追加 ID/时间 | mod::append_resolution | `crates/erp-integration/src/service/task_decision/mod.rs:49` | record_id→ResolutionId，difference.base.id→DifferenceId，latest/action/evidence/actor，最后 Instant::now；append 序号上限原文案映射 Conflict，其余 Logic。 |
| 正式任务动作时间 | action::transact_task_action record_activity | `crates/erp-processes/src/integration_resolution/task_decision/action.rs:75` | 本域已写完、subject 已更新之后，WorkItem CAS 之前。 |
| 正式任务完成时间 | complete::transact_task_completion complete_by_domain_command | `crates/erp-processes/src/integration_resolution/task_decision/complete.rs:64` | 本域已写完、subject 已更新之后，WorkItem CAS 之前。 |
| 审计 ID/时间 | mod::store_receipt resource_log_with_id | `crates/erp-processes/src/integration_resolution/task_decision/mod.rs:35` | JSON 成功之后才创建 audit；ID仍 receipt.receipt_id，audit factory 的内部时间仍在最后审计步骤产生。 |

本片不引入 next_id、不生成第二次 receipt ID。完整命令仍由 serde_json::to_vec 传 IntegrationCommandIdentity::new；原 key/operation 不追加 trim。`w29_action={};operation={};outcome={:?};evidence={}` 与 `operation={};reason_code={};terminal_evidence={};actor={}` 两种原摘要格式及 512 字节首错检查均保留。原中文字符串扫描无遗漏。完成事实借用给 runner 后按原值复制到 WorkItem/结果；不改变版本、时钟、写入或错误位置。

## 6. 回执、恢复与 wire token 证据

原三份 recover_* 相同控制流由真实生产 `execution::execute_with_receipt` 唯一实施：首次 replay 有值直接返回；无值才执行原事务准备；任意事务错误再执行一次原 replay；命中返回首次结果，无值返回原事务错误，恢复读取错误直接传播。事务成功不额外回放；不重试事务、不增加 8 次恢复、不按错误类别限制恢复。

真实 replay 继续先 audit find，再 `guard::decode_receipt`：actor/action/resource_type/resource_id/current actor→message 存在→JSON 可解析→fingerprint；不读当前 WorkItem。原身份冲突、消息缺失、JSON 不可解析的分类与文案不变。

下表比较 derive、字段类型与每个 serde attribute 的 Rust token；仅格式空白不计入。三种审计 action 字符串未改。

| 结构 | wire 合同 | token 对比 | token SHA256（before=after） |
| --- | --- | --- | --- |
| `ReceiptEnvelope` | f=fingerprint；r=result | True | `fbb79838a22ed3c02f40e9586a464c2ffc08b132e9cccbac9aacf03912ed5175` |
| `ActionReceiptMessage` | o=outcome；b optional+skip_none；e default+skip_empty | True | `576f290f1ba941b3b5d25368d941ddc682561355e28d1e43b696612a34555160` |
| `CompletionReceiptMessage` | e=terminal_evidence_reference | True | `c1f6277d2c73e8884dbe884ff9c61a4e9b4e98584f775377d7b6f72a03eb2624` |
| `DirectReceiptMessage` | s=status；t=is_terminal；o=outcome；b optional+skip_none | True | `d2a02ba9cf030348ee2d049a9079b503697dcc167b3d208b0acc3639ec345354` |

## 7. 测试保留与新增证明边界

原五个测试函数体 Rust token 全等；其 imports 和 production_source 的 include_str 路径更新到唯一 domain/process 归属，原正向与负向 source 断言均静态满足。它们仍只提供原级别的纯逻辑/静态分层证据。

| 测试定义 | 归属 | 合同／边界 |
| --- | --- | --- |
| `terminal_query_discovers_after_query_on_same_injected_executor` | `backend/crates/erp-integration/src/service/task_decision/action/tests.rs:100` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `authority_error_is_preserved_and_no_later_call_runs` | `backend/crates/erp-integration/src/service/task_decision/action/tests.rs:123` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `absent_or_unknown_original_does_not_discover_or_replay` | `backend/crates/erp-integration/src/service/task_decision/action/tests.rs:146` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `action_uses_one_executor_and_stops_at_every_failed_step` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:119` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `completion_receipt_follows_task_write_and_precedes_result` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:140` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `direct_task_guard_precedes_domain_read_and_each_failure_stops` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:161` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `committed_receipt_bypasses_closed_task_and_preparation_path` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:174` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `any_transaction_error_recovers_once_and_missing_receipt_keeps_original_error` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:193` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `replay_read_error_stops_initially_or_replaces_transaction_error_on_recovery` | `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs:219` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `receipt_identity_conflicts_precede_missing_or_unparseable_payload` | `backend/crates/erp-processes/src/integration_resolution/task_decision/guard/tests.rs:25` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `receipt_missing_message_and_invalid_json_keep_distinct_internal_errors` | `backend/crates/erp-processes/src/integration_resolution/task_decision/guard/tests.rs:43` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `same_key_different_payload_is_rejected_and_original_result_replays` | `backend/crates/erp-processes/src/integration_resolution/task_decision/guard/tests.rs:54` | 新增；直接调用真实 production runner/helper，替身不连接 DB。 |
| `receipt_never_contains_raw_idempotency_key` | `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs:25` | 原测试；函数体 token 保留。 |
| `terminal_evidence_allows_explicit_resolve_without_completing_action` | `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs:43` | 原测试；函数体 token 保留。 |
| `confirmed_no_result_allows_replay_only_for_error_task` | `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs:58` | 原测试；函数体 token 保留。 |
| `versions_flow_typed_from_prepared_without_reparse` | `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs:98` | 原测试；函数体 token 保留。 |
| `conclusion_and_append_are_owned_by_domain` | `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs:112` | 原测试；函数体 token 保留。 |

- execution/tests 的 TaskCommandPort 替身分别遍历 action/completion 每个失败步骤，核对精确前缀；direct 核对 NoTask→Domain→Receipt→Result，第一步失败不触碰领域读取。所有 I/O 步骤比较同一非零 `TestExecutor { _identity: u8 }` 的数据指针。
- receipt runner 测试直接调用生产 execute_with_receipt；覆盖已提交结果绕过关闭任务/准备路径、任意 Forbidden 事务错也恢复、无回执保留原错、首次/恢复读取自身失败。替身步骤不证明真实 MongoDB 事务提交结果未知行为。
- guard/tests 直接调用生产 decode_receipt；覆盖五类身份字段首错、缺消息/坏 JSON 的独立文案、同 key 异 payload 冲突、原值回放。
- domain action/tests 直接调用生产 query_action_fact；注入 Authority 并断言 query→discover 同一非零 Executor、两步分别失败后的前缀、NoResult/Unknown 不访问后续 provider。
- 已执行的是定向 rustfmt、原六叶输入哈希校验、原测试函数 token 比对、wire token 比对、中文字符串保留、include_str 目标存在及分层引用静态扫描。未将这些静态检查登记为 Rust 测试通过。

## 8. 源指纹

SHA256 均直接读取文件字节；after 文件如在统一门禁修复中改变，必须刷新本节及 JSON。

| before 源文件（453cd480） | SHA256 | after 状态 |
| --- | --- | --- |
| `backend/services/src/integration_ops/task_decision/action.rs` | `6664ecfd22274abfb846711af6dbe7fc5032776e3149fc5d2ae9708219b5e2ff` | 已删除，职责按第 2–4 节分配。 |
| `backend/services/src/integration_ops/task_decision/complete.rs` | `e0e9522992255b891bbd736b798ef249eb699e8b88b584477eafbe083074e5ff` | 已删除，职责按第 2–4 节分配。 |
| `backend/services/src/integration_ops/task_decision/direct.rs` | `09dface40133fe5520329a9c5c98d1a91e6ab0c549ae90a02662eb6684cad85a` | 已删除，职责按第 2–4 节分配。 |
| `backend/services/src/integration_ops/task_decision/guard.rs` | `c1c8af41017ba55eba1fe93f0d954cdb72c78d1f5400a987e3550e5caa5b09ce` | 已删除，职责按第 2–4 节分配。 |
| `backend/services/src/integration_ops/task_decision/mod.rs` | `21bab246823de02ab6108a9248b11d546bad8942d30b9ba61d2967db27c6bcb0` | 已删除，职责按第 2–4 节分配。 |
| `backend/services/src/integration_ops/task_decision/tests.rs` | `315deb250edbb4d9292d095e67393d710379b85ce7779af88746c04f6f488d54` | 已删除，职责按第 2–4 节分配。 |

| after 当前文件 | SHA256 |
| --- | --- |
| `backend/crates/erp-integration/src/service/task_decision/action/tests.rs` | `9318cc1b3d52bcd026ebb5e11a52a0217f07df9df793f12b0ba31163110010a7` |
| `backend/crates/erp-integration/src/service/task_decision/action.rs` | `e9d55bcd45da75edb31412b4654a6d8ac00f15fe0c426bd80b2c395bd67647d6` |
| `backend/crates/erp-integration/src/service/task_decision/complete.rs` | `9b37fc9c31c4dc63f6742d20caebb5f8099cbdafb2fdb406dd759d2dd6d99b1e` |
| `backend/crates/erp-integration/src/service/task_decision/direct.rs` | `c50eeb12962facae2e62deef45b35d0961c73491deb257f297a436019a3a7fcd` |
| `backend/crates/erp-integration/src/service/task_decision/guard.rs` | `e39c312bf446ba602049e6afbd57305f552e81cd0b7831cf494d02d078a33951` |
| `backend/crates/erp-integration/src/service/task_decision/mod.rs` | `6950c75ed6faf8abd2812e14e2a1ccff1f656d7d412d4920f1e68b0d04701f95` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/action.rs` | `c8045d15561b168c37d5df43f87a95b3ccdcca8f4f25ad585a42e40cd27dc628` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/complete.rs` | `d0b22b0dc4fe3953cc4ca3ddc8954f94f1938cba35ac2f3cf8d825e80a535aa2` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/direct.rs` | `6fe675e318176b25b527fbda0850216764ab3117cbbd19cb74df2e64880c7545` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/execution/tests.rs` | `acc254842da51ffdbe388e5daa74561d47f9d971b5a6ab116d7f133588d386b1` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/execution.rs` | `2552a09db3e1b4869dbac96ed3995767ab93b337ec932b5b472f46ca66332033` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/guard/tests.rs` | `a2cef77ad08150f6eda41db5cde9cf6fc8961097fa3f3ac6a84a552607208a99` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/guard.rs` | `9e0847ea7df0d66f1d8da2e019a4f9ff605fc60651524abd9def2054642f930f` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/mod.rs` | `21c048cd63259db42b3175fda80f99f472142e67c1bf581acedb70d7323f1eae` |
| `backend/crates/erp-processes/src/integration_resolution/task_decision/tests.rs` | `b469140f2f564b3e2ea0630be0a9edb53ce44b611db987b0548811f06558257f` |
