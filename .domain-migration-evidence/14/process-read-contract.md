# 阶段 14 分片 B：交付与验收合同

## 1. 输入与责任

- 源码输入：`453cd48082793b8f40e5afa37d5d225a747fd1b0`。
- 唯一实施树：`/private/tmp/erp-domain-crate-14-integration`。
- B 独占旧 integration_ops 服务的 14 个非 DTO 文件；其中 task_decision 六叶委托 `/root/finance_receivable/purchase_lifecycle`。另接收 A 原 w29_work_items 的两工厂与三个完整工厂测试。
- A 持有领域根、DTO、Port、责任 Spec、实体和持久化；C 持有供应商执行根；root 持有 Cargo、lib、HTTP、AppState 和统一门禁。
- B 已按输入快照逐字节核对并删除八个直属旧服务文件，子分片已删除 task_decision 六叶。旧 IntegrationOpsService 不保留 façade。

## 2. 实际入口及注册

| 消费方 | 实际类型与构造 | 唯一职责 |
| --- | --- | --- |
| HTTP 七写 | `erp_processes::integration_resolution::IntegrationResolutionProcess::new(Database, Arc<dyn IntegrationEvidenceAuthority>)` | register_inbox_message、write_back_inbox_result、create_error_task、create_difference、apply_task_action、complete_task、decide_difference。原参数和响应 DTO 保持。 |
| HTTP 两详情 | `erp_read_models::integration_center::IntegrationCenterReadService::new(Database, Arc<dyn IntegrationEvidenceAuthority>)` | error_task_detail、difference_detail。无新增 actor 或查询授权步骤。 |
| HTTP 四读 | `erp_integration::service::IntegrationOpsService::new(Database)` | inbox_message_list、inbox_message_detail、error_task_list、difference_list。 |
| AppState 单例 | `erp_processes::integration_resolution::evidence_adapter::MongoIntegrationEvidenceAuthority::new(Database)` | 唯一生产 EvidenceAuthority；Process 与 RM 必须 clone 同一 Arc 注入。 |
| C 正式责任生产 | `crate::integration_resolution::producer::error_work_item(&IntegrationErrorTask, &str) -> services::Result<WorkItem>` | pub(crate) 单份工厂，不暴露到旧服务，不复制 WorkItem 构造。 |

`erp_integration::service::task_decision` 由 A 根额外公开注册；B 的 Process/RM 私有子模块已实际注册。B 未改全局 Cargo/lib、HTTP/AppState、历史 tests 或 MongoDB。

## 3. 逐文件归属

| 旧文件 | 实际领域实现 | 实际 Process / RM 实现 |
| --- | --- | --- |
| services/integration_ops/mod.rs | A service 根仅持有 db，四读方法在本域叶 | Process 根持有 db/evidence；RM 根持有 db/evidence。原业务约束注释归 Process 根。 |
| inbox_message.rs | list/detail；prepare_registered_inbox_message；apply_processed_outcome/apply_failed_outcome；prepare_failed_message_task；persist/update 与失败组合仓储写入 | inbox_message.rs 两写根；register/source 存在性、Audit 与事务；failed 正式责任通过 creation_writes。 |
| error_task.rs | error_task_list、ensure_message_exists、prepare_error_task、persist_error_task | error_task.rs 创建根；RM error_task.rs 详情、正式关联和动作投影。 |
| reconciliation_difference.rs | difference_list 单页批量最新决定；prepare_difference、persist_difference；原两个测试 | 创建根；RM reconciliation_difference.rs 详情、完整历史、正式关联和动作投影。 |
| evidence.rs | service/evidence.rs 唯一策略/原因/View 映射、kinds、完成与直接原因校验、逐证据验证、引用串联、grammar 错误映射；原三个测试 | evidence_adapter.rs 唯一跨域 Mongo 权威实现和补偿发现。A 持有原 Port/facts。 |
| producer.rs | 消费 A 责任 Spec/注册表，不添加 owner/trim 校验 | producer.rs 保留 ID/时钟注入；work_item_factory.rs 唯一 Spec→真实 WorkItem 装配；原三个 producer 测试。 |
| transaction.rs | 不进入领域 | Process transaction.rs 原 run_audited 模板。 |
| validation.rs | service/validation.rs 原 ensure_version | Process 直接消费窄公开函数。 |
| task_decision/{mod,action,complete,direct,guard,tests}.rs | service/task_decision 下真实动作、证据、领域 guard、完成与不可变决定追加 | integration_resolution/task_decision 下真实 WorkItem/权限/回执/审计及执行器步骤；原五测试完整迁移。 |
| entities/integration_ops/w29_work_items.rs 两工厂片段 | A 的 error_responsibility/difference_responsibility 只生成责任 Spec | work_item_factory.rs 显式一一映射 kind/priority，保留 SystemRule/due_at=None，最后调用 WorkItem::new_at；原三个工厂测试完整迁移。 |

新增 `creation_writes.rs` 是三条实际生产根共同使用的步骤执行器。真实 Mongo 实现分别调用本域写入、WorkItem.create、Audit.create。它不包含第二份领域判断或第二次事务。

## 4. 强制顺序与错误边界

1. register：validate → SourceRegistry 的 NoTransaction 存在性 → received_at → ID/InboxMessage::received → audit → 原 session 内 message.create → audit.create。返回已构造实体，不回读，不增加回执恢复。
2. write_back：Prepared::prepare(&req, Instant::now()) → NoTransaction 消息 → ensure_version；processed 在原状态 update 后生成审计，事务内 inbox CAS → audit。failed 先更新消息状态，再 ID/任务构造，只有 summary Some 才 record_attempt，再 WorkItem/audit；事务内 error insert → inbox CAS → WorkItem → audit。
3. create_error：validate → 可选 inbox 存在性 → task ID/构造 → WorkItem ID/时钟/原规则 → audit → 同 Executor task → WorkItem → audit。
4. create_difference：validate → clone owner → ID/实体 new，仍映射 ValidationError → producer 第一次注册预检，仍 BusinessLogicError → WorkItem ID/now → 工厂内第二次注册查询 → WorkItem::new_at → audit → 同 Executor difference → WorkItem → audit。
5. action/complete：完整命令 identity → receipt 优先回放 → 原 shared_rbac_service 时点 → Prepared → WorkItem 版本/状态/当前 owner/正式关联 → 真实 ensure_domain_decision_access → 领域动作与写入 → WorkItem subject/activity 或完成 → WorkItem CAS → receipt audit。原无操作资格函数和 if false 分支保持。
6. direct：receipt 回放 → Prepared → 正式任务存在性（含关闭任务） → difference → 最新决定 → version/open → 原动作/证据/领域追加 → receipt audit。
7. 三个强命令任何事务错误均进行一次 fresh receipt 回读；命中返回首次结果，缺失返回原错误，回读自身错误保留原 `?` 的覆盖语义。没有新增重试。
8. 两个详情保留原查询顺序，发现多个正式关联仍返回原 ConflictError。差异列表保持单次 find_latest_by_differences，不产生逐行查询。
9. adapter 五方法全接原 Executor，不自开事务，不外发请求。query_original 的 processed 优先、无结果条件和首条发现结果保持。replay_original 只重排原 inbox。verify_reattribution 仍失败关闭。逐类型验证顺序、独立复核、正式关联和 canonical 编码保持。
10. discover_evidence 保留 inbox → 既有差异证据 → 补偿顺序；补偿按 customer refund → supplier refund → supplier fact 的原条件与首次命中提前返回。旧 supplier 事实只能在 adapter 经旧仓储读取，等待阶段 16 归属闭合。

## 5. 原测试与新增验证

| 实际归属 | 原测试 | 新测试 | 验证对象 |
| --- | ---: | ---: | --- |
| domain evidence | 3 | 3 | 原策略/原因/分层断言；逐证据请求顺序、相同非零 Executor、第二项首错停止、空证据先拒绝。 |
| domain difference | 2 | 0 | 原批量查询及实体参考不变量断言。 |
| process inbox/producer/factory | 7 | 0 | 原创建形状、责任政策及真实 WorkItem 完整字段。 |
| process creation_writes | 0 | 2 | 三个真实步骤复用同 Executor；每步失败只执行此前前缀并保留原错。 |
| delegated task_decision | 5 | 12 | 三生产 runner 的同 Executor/失败前缀、receipt 优先/恢复、identity/payload 异常顺序、domain query→discover 真实注入。 |
| 合计 | 17 | 17 | 34 个测试定义；本分片不执行 Cargo 或数据库。 |

原测试名称与断言保留。旧 source include 类型测试更换为真实领域/Process 所属源组合，不删除正/负断言。测试定义和静态审查不能替代 root 的统一执行结果。

## 6. 源码证据与验收

- `/private/tmp/integration14-b-semantic-review.py` 可重读当前文件并生成 `/private/tmp/integration14-b-semantic-review.json`。
- B 直属八个旧文件与输入提交逐字节相同后删除；JSON 记录原 SHA、输入一致性及删除结果。
- 55 项原函数/原测试对照全部规范化 token 相等。允许差异仅是注释/空白、rustfmt 尾逗号、窄可见性、明确 crate 路径、Database→adapter.db 与唯一 Arc 证据注入；不规范化业务字符串、错误、状态、serde 或步骤顺序。
- 七写中拆分的四个普通根通过上节顺序合同和真实 creation_writes 测试登记，不伪称整个函数原文相等；三个强命令另见子分片 `/private/tmp/integration14-task-decision-result.json`。
- 17 个 B 直属新文件 SHA 已记录。子分片完成后其 SHA 以子证据为准；root 最终源码冻结后必须重捕获本 JSON，不能用中间树 hash 代替封存 source commit。
- B 已仅定向 rustfmt，`git diff --check` 返回成功。root 负责统一 workspace check、clippy、领域边界/静态合同与所有测试；本文件不宣称这些门禁已通过。
