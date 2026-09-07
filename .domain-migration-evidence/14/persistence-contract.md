# 阶段 14 A 集成领域迁移交付合同

## 1. 输入、所有权与验证边界

输入固定为 `453cd48082793b8f40e5afa37d5d225a747fd1b0`，唯一实施树为 `/private/tmp/erp-domain-crate-14-integration`。A 已迁移并删除 27 个旧文件，目标为 `erp-integration` 的 entity/repository/indexes/dto/ports/error 与 service 根。旧文件删除前逐项核验准备合同 SHA-256；未修改共享 Cargo/lib/HTTP/AppState/旧注册根、B 的 service 叶、历史 tests/**。

实施与核销必须使用下列产物：

- [逐源文件、生产函数、测试、schema 与源 hash](/private/tmp/integration14-a-result.json)：27 文件，258 原生产函数，102 个原文件内测试的单项去向，36 个当前 A 源文件 hash。
- [输入文件清单](/private/tmp/integration14-a-before.json)：27 个旧文件及输入提交。
- [静态核销脚本](/private/tmp/integration14-a-evidence.py)：仅读取 git 输入对象及当前源文件，不运行 Cargo/Mongo。
- [工厂测试移交原文件](/private/tmp/integration14-w29-factory-test-handoff.rs)：完整原 W29 文件，3 个原工厂测试已经 B 接入真实流程工厂。

本分片只执行定向 rustfmt 与静态源核销。Cargo 编译、Clippy、单元测试执行与全域依赖门禁由唯一集成人运行；本报告不将静态核销表述为测试通过或真实 MongoDB 验证。尚未执行真实数据库查询、索引创建或事务。

## 2. 公开出口与消费者接入

| 责任 | 实际出口 |
| --- | --- |
| 原实体、Data/Update、状态与 ErrorClass、证据 grammar、决策/关闭政策、命令身份 | `erp_integration::entity::integration_ops::{原公开符号}` |
| W29 固定责任事实 | `erp_integration::entity::integration_ops::{IntegrationResponsibilitySpec,IntegrationResponsibilityKind,IntegrationResponsibilityPriority,error_responsibility,difference_responsibility}` |
| 原 W29 查表函数和常量 | 上述 entity 根保留 `error_owner_role,error_work_item_type,error_priority,difference_owner_role` 及 8 个原常量；两个任务 enum 返回类型是本域窄 enum |
| DTO 与 Prepared | `erp_integration::dto::{原 dto.rs 全部 public use}` |
| 权威事实端口 | `erp_integration::ports::evidence::{EvidenceSubject,VerifiedEvidence,OriginalResultFact,EvidenceFuture,IntegrationEvidenceAuthority}` |
| 仓储入口 | `erp_integration::repository::IntegrationOpsExt` |
| Filter/Row/聚合仓储 | `erp_integration::repository::integration_ops::{InboxMessageFilter,InboxMessageRow,IntegrationErrorTaskFilter,IntegrationErrorTaskRow,ReconciliationDifferenceFilter,ReconciliationDifferenceRow,ResolutionHistoryRow,IntegrationOpsRepository}` |
| 4 owned 类型 | `erp_integration::repository::owned::{InboxMessageRepository,IntegrationErrorTaskRepository,ReconciliationDifferenceRepository,ReconciliationDifferenceResolutionRepository}` |
| 索引唯一入口 | `erp_integration::indexes::ensure(&Database) -> persistence_core::Result<()>` |
| 本域 Service | `erp_integration::service::IntegrationOpsService::new(Database)` |

`repository::extensions` 保持私有，仅公开重导出 trait。旧域根不保留 façade。DTO 私有子树仍私有；只有约定的 3 个 command validate、4 个 enum as_str、1 个 as_non_terminal_action 从 pub(crate) 提升为 pub。三个列表 normalized、Prepared parse、decimal_version 没有扩大公开范围。

Service 根只持 Database，登记 `evidence/inbox_message/error_task/reconciliation_difference/validation/task_decision` 六个叶；真实叶实现属于 B 与其获授权子分片。A 不重复实现决定 append、证据 adapter 或 WorkItem 工厂。

EvidenceAuthority 的 query_original/replay_original/verify_reattribution/verify_evidence/discover_evidence 五方法全部保留原参数生命周期、串行 Future 形状、调用方 `&mut dyn Executor`。EvidenceSubject 两个真实构造函数函数体 token 与旧源一致。Process/ReadService 共用上层注入的同一个 Arc，只有 B 的 MongoIntegrationEvidenceAuthority 实现真实跨域加载。

## 3. W29 拆分与错误次序

原两个 `new_*_work_item` 的固定责任映射移入 Spec，实际 WorkItem::new_at、SystemRule、due_at=None、ID/时钟留 B 的 `erp_processes::integration_resolution::work_item_factory`。Spec 不访问数据库、不生成 ID、不读时钟、不 trim 或验证 owner。error_responsibility 无失败分支；difference_responsibility 只执行原 difference_owner_role 注册校验。

error 主题版本取原 task.base.version；difference 初始主题版本始终为字符串 0。注册匹配允许 trim/ASCII lowercase，冻结 reason_code/impact_summary 仍使用原差异字符串。新增真实生产函数测试固定空白 owner 延迟验证、历史分类原字节保留、当前 task 版本与固定 difference 版本，以及未知差异首错。

B 必须保持 error 的 next_id→now→factory/spec→WorkItem 校验，difference 的原责任预检→next_id→now→factory 内第二次注册→WorkItem 校验。原 3 工厂测试仍驱动 process 中真实 WorkItem 工厂；不能替换为仅检查 Spec。

## 4. 持久化与索引合同

集合固定为 `inbox_messages`、`integration_error_tasks`、`reconciliation_differences`、`reconciliation_difference_resolutions`，真实 owned 数为 4。全部 owned 的 generic 委托与专用查询原生产函数体保持同构，实体字段/serde、Filter/Row 字段与 query pipeline 保持输入；仅发生命名空间、必要可见性和两个长签名的 rustfmt 尾逗号变化。

索引 ensure 次序必须为入站→错误任务→差异→决定，共 9 个：

| 集合 | 固定索引名 |
| --- | --- |
| inbox_messages | uk_inbox_messages_identity；uk_inbox_messages_business_fact；idx_inbox_messages_backlog |
| integration_error_tasks | uk_integration_error_tasks_message_class；idx_integration_error_tasks_work_queue |
| reconciliation_differences | uk_reconciliation_differences_object；idx_reconciliation_differences_object_time |
| reconciliation_difference_resolutions | uk_reconciliation_difference_resolutions_no；idx_reconciliation_difference_resolutions_difference |

inbox 两个唯一键保持全局唯一；错误任务 partial unique 保留 message_id BSON string 且 status in pending/auto_retrying/manual_required；差异对象全局唯一与决定序号追加唯一保持。全部 key 的顺序、name、unique 与 partial_filter 原生产 token 一致。

`IntegrationOpsRepository::create_error_task_with_message_failure` 保持 error task insert_one→InboxMessageRepository.update/CAS，两个调用传原同一 Executor，第二步失败原样返回；该方法不创建事务。owned update 继续委托同一 persistence-core Repository::update，未新增无会话写。

`find_latest_by_differences` 保持首次出现顺序去重→空输入直接空 map→单次未删除 `$in` 查询→resolution_no 最大者胜出。没有决定记录的差异不上报默认实体。单差异 latest、三个列表 filter/排序白名单/页码、WorkItem 定位查询与所有原行投影均保留。

## 5. 原测试与 schema 核销

原 A 文件包含 102 个测试：entity 76、database 11、DTO 15。3 个 W29 工厂测试移交 B 后，A 保留责任为 99。4 个 BSON 测试整段从 entity 移至 repository::serialization_contract，复用原 fixture，完整保留断言，不引入 entity→BSON 或 domain dev→其他业务域。

99 个 A 测试与 3 个交 B 工厂测试，原函数体在命名空间与消费方 enum 名替换后全部 token 相等。A 新增 5 个真实事实合同测试，另保留 root 初始化 error.rs 中 2 个新增错误映射测试，合计本分片当前 106 个测试声明。新测试不代替任何原测试。

258 个原生产函数已逐符号登记：256 个函数体在命名空间与消费方 enum 名替换后相等；2 个 W29 工厂有明确 Spec/process 去向。25 个完整生产声明检查全部核销，覆盖持久化 schema、DTO serde/validator 与查询实现。两个 owned 签名仅有 rustfmt 所加泛型/receiver 尾逗号，分别在 JSON 独立标记，未用宽泛归一化隐藏差异。

## 6. 逐文件迁移表

| 旧源文件 | 实际目标 | 原测试数 |
| --- | --- | ---: |
| `backend/entities/src/integration_ops/decision_policy.rs` | `backend/crates/erp-integration/src/entity/integration_ops/decision_policy.rs` | 6 |
| `backend/entities/src/integration_ops/direct_conclusion.rs` | `backend/crates/erp-integration/src/entity/integration_ops/direct_conclusion.rs` | 1 |
| `backend/entities/src/integration_ops/error_classification.rs` | `backend/crates/erp-integration/src/entity/integration_ops/error_classification.rs` | 4 |
| `backend/entities/src/integration_ops/evidence_reference.rs` | `backend/crates/erp-integration/src/entity/integration_ops/evidence_reference.rs` | 10 |
| `backend/entities/src/integration_ops/inbox_message.rs` | `backend/crates/erp-integration/src/entity/integration_ops/inbox_message.rs` | 10 |
| `backend/entities/src/integration_ops/integration_error_task.rs` | `backend/crates/erp-integration/src/entity/integration_ops/integration_error_task.rs` | 19 |
| `backend/entities/src/integration_ops/mod.rs` | `backend/crates/erp-integration/src/entity/integration_ops/mod.rs` | 1 |
| `backend/entities/src/integration_ops/reconciliation_difference.rs` | `backend/crates/erp-integration/src/entity/integration_ops/reconciliation_difference.rs` | 7 |
| `backend/entities/src/integration_ops/reconciliation_difference_resolution.rs` | `backend/crates/erp-integration/src/entity/integration_ops/reconciliation_difference_resolution.rs` | 10 |
| `backend/entities/src/integration_ops/w29_close.rs` | `backend/crates/erp-integration/src/entity/integration_ops/w29_close.rs` | 3 |
| `backend/entities/src/integration_ops/w29_work_items.rs` | `backend/crates/erp-integration/src/entity/integration_ops/w29_work_items.rs` | 5 |
| `backend/services/src/integration_ops/dto/common.rs` | `backend/crates/erp-integration/src/dto/common.rs` | 1 |
| `backend/services/src/integration_ops/dto/error_task.rs` | `backend/crates/erp-integration/src/dto/error_task.rs` | 1 |
| `backend/services/src/integration_ops/dto/inbox_message.rs` | `backend/crates/erp-integration/src/dto/inbox_message.rs` | 1 |
| `backend/services/src/integration_ops/dto/prepared_decision.rs` | `backend/crates/erp-integration/src/dto/prepared_decision.rs` | 7 |
| `backend/services/src/integration_ops/dto/prepared_inbox.rs` | `backend/crates/erp-integration/src/dto/prepared_inbox.rs` | 2 |
| `backend/services/src/integration_ops/dto/reconciliation_difference.rs` | `backend/crates/erp-integration/src/dto/reconciliation_difference.rs` | 1 |
| `backend/services/src/integration_ops/dto/task_decision.rs` | `backend/crates/erp-integration/src/dto/task_decision.rs` | 2 |
| `backend/services/src/integration_ops/dto.rs` | `backend/crates/erp-integration/src/dto/mod.rs` | 0 |
| `backend/database/src/indexes/integration_ops.rs` | `backend/crates/erp-integration/src/indexes/integration_ops.rs` | 3 |
| `backend/database/src/repository/extensions/integration_ops.rs` | `backend/crates/erp-integration/src/repository/extensions.rs` | 0 |
| `backend/database/src/repository/integration_ops.rs` | `backend/crates/erp-integration/src/repository/integration_ops.rs` | 4 |
| `backend/database/src/repository/integration_ops/difference_resolution_batch.rs` | `backend/crates/erp-integration/src/repository/integration_ops/difference_resolution_batch.rs` | 4 |
| `backend/database/src/repository/owned/inbox_message.rs` | `backend/crates/erp-integration/src/repository/owned/inbox_message.rs` | 0 |
| `backend/database/src/repository/owned/integration_error_task.rs` | `backend/crates/erp-integration/src/repository/owned/integration_error_task.rs` | 0 |
| `backend/database/src/repository/owned/reconciliation_difference.rs` | `backend/crates/erp-integration/src/repository/owned/reconciliation_difference.rs` | 0 |
| `backend/database/src/repository/owned/reconciliation_difference_resolution.rs` | `backend/crates/erp-integration/src/repository/owned/reconciliation_difference_resolution.rs` | 0 |

上述表中 w29_work_items 的 5 个原测试按 A 2/B 3 拆分；四个 entity BSON 测试的实际目标以 JSON 单项 test map 为准。共享注册由 root 按实际公开出口完成；本分片不得自行公开整个私有子树消除编译错误。
