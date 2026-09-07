# 阶段 14 幂等与事务源码核验记录

- 输入提交：`cb159e5bf874e54254ea6fc447347627572d1a16`；输入源码提交：`453cd48082793b8f40e5afa37d5d225a747fd1b0`。
- 候选：`/private/tmp/erp-domain-crate-14-integration`；源码绑定：`b274cebaefad13c1d0216a7b2252a4cecab5413f`。
- 范围：37 个 source-map 文件、4 个拥有仓储来源、15 个额外调用方；实际函数 380 个，其中主体规范化一致 328 个，改变/提取后逐 provider 核验 52 个。
- 结论：本记录范围内未发现原查询、首错、ID/时钟时点、回执、权限、写序、同 Executor 或网关事务边界的业务漂移。
- 证据级别：实际源码对照与生产 provider 追踪。未运行 Cargo、MongoDB、外部 gateway 或历史 tests。**真实数据库运行未验证**。
- 每个实际 qualified symbol 的 before/after 路径、行、文件 SHA256、函数源码 SHA256、主体 token SHA256 与 provider 清单见同名 JSON；不是扫描占位符。

## register：入站登记与处理成功回写

- 登记：Validate → source_systems.find_by_id(NoTransaction)，仅检查存在 → received_at 缺省 now_secs → prepare_registered_inbox_message 内唯一 next_id → InboxMessage::received → audit 构造 → 原根事务。源系统缺失仍 NotFound(来源系统不存在)。
- 登记事务仅 inbox create → audit create，返回原构造 message.into，不预查 inbox 身份、不重读详情、不新增命令回执恢复。
- write_back 首先以 Instant::now 准备 PreparedWriteBackOutcome，然后 NoTransaction 读取消息 → ensure_version；Processed 分支实体 update → audit 构造 → 单一根事务内 inbox CAS → audit，返回存储后实体。

## creation：失败回写、错误任务与差异创建

- Failed 回写：先更新 message Failed/processed_at None → prepare_failed_message_task 内 next_id/原责任矩阵 → 仅 attempt_summary Some 时 record_attempt(attempt_at) → W29 WorkItem → audit；没有提前生成第二个 ID 或时钟。
- CreatedFact::FailedMessage → MongoWrites::domain → persist_error_task_with_message_failure → 同一 integration repository 的 task insert 后 inbox CAS；随后相同 Executor 的 WorkItem create → audit create。
- create_error_task：Validate → 可选消息存在性 NoTransaction，原 NotFound(关联消息不存在) → 任务 ID/实体 → WorkItem → audit → task/work_item/audit 顺序。
- create_difference：Validate → clone owner → 差异 ID/实体，原实体错误转 ValidationError → difference_work_item 预检/工厂 → audit → difference/work_item/audit 顺序。
- CreationWrites 的 execute 三步均用收到的同一 executor，各 await? 首错停止；MongoWrites 每分支调用真实仓储/本域写入，无嵌套事务。

## receipt：七命令中的三条 W29 幂等强命令

- apply_task_action、complete_task、decide_difference 先 Validate；direct 再验证 path_id==difference_id；完整命令 serde_json::to_vec 后构造 IntegrationCommandIdentity。原 idempotency_key/operation_id 不再 trim，w29_ 摘要与完整 payload 指纹保持原唯一算法。
- execute_with_receipt 精确保留首次 replay 命中即返回；只有未命中执行写；任何写错误（包括提交 Unknown、事务初始化/业务/CAS 错误）只执行一次 fresh replay；命中返回原结果，未命中返回原错误，恢复查询自身失败仍 ? 返回恢复错误。
- 三次 recover_* 原函数由同一 execute_with_receipt 的 Err 分支承接，没有收窄为仅 DuplicateKey，也没有重试事务。replay_receipt 仍独立 NoTransaction 查询审计。
- decode_receipt 从真实 replay_receipt 调用：先 receipt.matches_receipt 四身份字段和当前 actor → message Some → serde JSON → fingerprint，相同错误类别与文案；不先读当前 WorkItem。
- action/complete 的 shared_rbac_service 仍在首次回执回放之后、PreparedWorkItemTarget 之前；direct Prepared 仍位于根事务闭包内。构造器只 clone Database/Arc，不执行查询/权限。
- ReceiptEnvelope f/r；Action o/b/e（b Option skip，e default+empty skip）；Completion e；Direct s/t/o/b，源字段/serde 属性逐字保持。三条 audit action 常量与 resource 身份未改。

## task：正式任务绑定、真实授权和本域写入

- load_bound_work_item：WorkItem find → task/subject 版本 → Open → 当前 owner → 原 no-op ensure_actor_eligible → 业务正式关联；原 if false 分支保留，不启用新授权政策。
- 错误任务关联仍先读取任务，ResultUnknown 对应 IntegrationResultUnknown，其他 BusinessException，再核对类型/对象/id；关联加载的新 helper 不检查终态，之后原领域 load_error_task 才拒绝终态，未提前首错。
- ActionCommand/CompletionCommand::authorize 仍调用 workflow_compose::work_item_service(...).ensure_domain_decision_access(actor,item,executor)，沿 run_action/run_completion 的同一 Executor，未用 DTO allowed_actions 代替对象授权。
- run_action：load → authorize → domain apply（真实 domain 任务/差异写）→ subject_version/record_activity(now) → WorkItem CAS → task_action_result → receipt。
- run_completion：load（先 as_non_terminal_action）→ authorize → domain complete/权威证据 → subject_version/complete_by_domain_command(now) → WorkItem CAS → receipt → completion_result。
- domain action/error：load/terminal → subject check → evidence/action → summary → record_attempt(now) → task CAS → next subject。difference：difference → latest → subject → open → fact → append(now) → immutable resolution create。
- domain complete/error 与 complete/difference 保留版本/终态、策略、证据按序验证、reference/resolution 构造、领域状态/写入，最后返回给 WorkItem transition；append_resolution 的序号上限 ConflictError 映射与 Instant::now 位置未改。
- direct run_direct：ensure_no_work_item（任何正式关联，包括关闭/完成）先于 difference 查询；execute_direct_decision 随后 difference → latest → version → open → fact → append(now) → resolution create → receipt → direct_result。每步首错停止。

## evidence：单份跨域权威证据与失败关闭

- 原 Database 的 IntegrationEvidenceAuthority 实现迁至唯一 MongoIntegrationEvidenceAuthority，具体读取仅 self 改 self.db，query_original/replay_original/verify_evidence/discover_evidence 的分支、顺序和原 Executor 未变。
- query_original 优先 message_id：Processed 且有 processed_at 返回 canonical；符合原可重放注册、Failed/ToManual、无 processed_at、无 supplier_refund_fact 才 NoResult；其他情况 discover 第一条或 Unknown。
- replay_original 仍重新读取原 inbox、校验无结果及注册条件、仅更新同 inbox Received、编码更新后 version/business_fact_key；没有供应商 gateway 调用。verify_reattribution 仍固定失败关闭。
- verify_evidence 保留 grammar → type/kind → 存在性 → Posted/Processed/终态 → DistinctReview 独立处理人 → 正式关联 → canonical；supplier_refund_fact 绑定 fact/inbox/order/event，差异独立复核必须同差异 AddEvidence、有证据且非本处理人。
- discover_evidence 顺序 inbox → 差异 AddEvidence → discover_compensation；后者 customer_refund、supplier_refund、supplier_refund_fact 的条件/提前返回保持；没有并发/排序/N+1 改写。
- verify_evidence_refs 空数组原 ValidationError，非空按请求原顺序调用 injected authority.verify_evidence(subject,evidence,actor,同 executor)，首错停止；grammar→ValidationError 继续单一定义。

## read：四本域读、两跨域详情

- inbox list/detail、error_task_list、difference_list 函数主体在导入重定位后保持：Validate/normalized/filter/分页排序、时间秒数和 Option 投影未变。difference list 仍一批 find_latest_by_differences，缺决定仍 None/0。
- error detail：task → resolution clone → 唯一 WorkItem 查询（多条 Conflict）→ subject → 同 injected evidence discover(NoTransaction) → 原 policy/action projection；仍无 actor 参数或新增对象授权。
- difference detail：difference → 原完整 search_resolutions → history.last 状态/version → WorkItem 唯一关联 → discover → policy/registry；终态与是否有任务决定原政策显示分支，未改变查询优先级。

## factory：W29 ID/时钟与完整 WorkItem 工厂

- producer::error_work_item：WorkItemId(next_id()) → Instant::now() → factory → error_responsibility → 原 WorkItem::new_at。Spec 不生成时钟/ID、不验证/trim owner。
- difference_work_item 先 difference_owner_role 注册预检并映射 BusinessLogicError，然后 next_id → now → factory 内 difference_responsibility 的第二次原注册检查 → WorkItem::new_at；没有合并两次查询或提前验证 owner。
- Spec→WorkItemData 显式映射原类型/priority，完整 object/subject/role/org/user/reason/impact 保持；SystemRule、due_at None；error 八类矩阵与 difference 注册类别保持原样。

## jobs：连接后台事务外网关与结果写序

- execute_connection_command 和 create_health_job/create_catalog_job 主体保留：命令权限/identity/receipt → 创建意图事务内 connection/version/capabilities/blockers → job next_id；health 再构造确定性 run/capability snapshot → job create → run create → Processing receipt；catalog 无 run。
- process_connection_job 仍 NoTransaction governance_job，不存在 NotFound，terminal 直接返回，按两原 job_type 分发。HealthExecution::start await start_health_job 完成根事务后 invoke；gateway 参数只含事实，没有 Executor。
- start_health_job：job 重读/ Pending → run → connection → 单次 now → job.start → run.start → job CAS → run CAS，同一 session。invoke 在 start 返回后 MonotonicInstant::now → gateway.health_check → elapsed millis/u64::MAX 饱和。
- finish_health_job 重读 job → run → connection → now；技术配置变化优先覆盖任意 gateway outcome，以 ResultUnknown/TECHNICAL_CONFIG_CHANGED 失败并写 W29，不写 connection health。普通 Err settle → record_health Failed → touch → connection CAS → W29。成功 progress(1,0,0)/succeeded/run.succeed → Healthy/touch/connection CAS。共同尾部 job CAS → run CAS → settle audit 构造/写。
- settle_health_failure 先 job progress(0,0,1)/failed；ResultUnknown run.mark_unknown，其他 run.fail；不新增重试。失败码/摘要和 audit action/id/message 原样。
- CatalogExecution::start：domain_job_id 必填首错 → SupplierApiService::load_connection(NoTransaction) → start_background_job 根事务；只重读 job/Pending/now/start/CAS。调用原预读 connection 的 gateway.catalog_sync 后才 finish。
- finish_catalog_job 只重读 job → now → Err progress/failed/W29 或 Ok progress/succeeded → job CAS → audit，不新增 connection 读取/配置检查/run/health更新。
- persist_health_failure_task 先 w20-error-digest 任务构造 → 唯一 producer WorkItem(next_id,now) → w20-work-audit-digest 审计构造，之后 MongoFailureWrite 沿原 executor task create → WorkItem create → audit create，首错不继续。
- ConnectionJobExecutionPort::execute 的 start 失败直接返回，不调用 gateway/finish；gateway 的 ClassifiedError 作为 outcome 原样传入 finish；finish 错误直接透传。没有新会话重放/外部成功替身。

## close：W29 关闭实际调用链

- erp_workflow::service::work_item::close 根整文件与输入字节一致；managed access/item/general guard/request/receipt/replay/scope/object/replacement 先后保持；close_with_domain_evidence 仍先 closed_at、item.close、evidence/audit，再单一事务内 facts.persist_w29_close → WorkItem CAS → audit；原失败恢复保持。
- WorkflowObjectFacts::prepare_w29_close 主体不变；persist_w29_close 仅 IntegrationOpsExt/实体类型路径改变。replacement 先读及验证，之后按对象分类；错误任务原 class→WorkItem 类型校验、Closed/ResolutionType::Close/原 evidence/closed_at/任务 CAS。
- 差异分支原 difference → latest → terminal reject → next_resolution_no → CommandFingerprint(receipt_id) 的 w29-close ID → replacement 对应 CloseDuplicate 否则 CloseMisrouted → grammar/new_close_evidence → create，相同 Executor、分类与首错。

## wiring：AppState、HTTP spawn 与范围外消费者

- AppState::new_with_connectors 唯一构造一个 Arc<MongoIntegrationEvidenceAuthority>，integration_resolution()/integration_center() clone 同一 Arc；readmodels 仅持 Port，未依赖 processes。
- SupplierApiService::new(db) 保留 UnavailableSupplierReferenceRegistry/rbac None；AppState 仍按原顺序 with_reference_registry → with_rbac。后台 Process 注入原 external_connectors.supplier_api Arc，没有新 connector。
- supplier_api_connection_command 仍先 await 创建命令，再 Some(job_id) 时 tokio::spawn，捕获 state/actor/connection_id/job_id，原错误日志与即时响应保持；spawn 内实际调用迁到 execution process。readiness 原函数主体不变。
- 集成 HTTP 七写调用新 process、两详情调用新 RM、四本域读调用新 domain；请求/actor 参数、响应包装/状态、permission 宏及路由保持。
- supplier_fulfillment place/cancel/refund_result/gateway、services work_item facts、RM workbench facts 只换集成类型/Ext导入；原生产函数主体保持。W26 原工厂不换 W29 工厂；原回调唯一身份查询、已存在返回、数量限制和同 Executor 写序保持。

## 逐改变/提取符号核销

所有表项结论均为“已沿实际 provider 核验，未发现行为漂移”；合同组中的步骤和完整 provider 哈希为该结论的限定范围。

| before actual qualified symbol | after actual provider | 合同组 | after 文件 SHA256 前 16 位 |
| --- | --- | --- | --- |
| `web_api::app_state::impl<AppState>::new_with_connectors` | `web_api::app_state::impl<AppState>::new_with_connectors` | wiring | `a55cb3e804fbcba4` |
| `web_api::app_state::impl<AppState>::supplier_api_service` | `web_api::app_state::impl<AppState>::supplier_api_service` | wiring | `a55cb3e804fbcba4` |
| `web_api::core::handler::integration_ops::inbox_message_register` | `web_api::core::handler::integration_ops::inbox_message_register` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::inbox_message_write_back` | `web_api::core::handler::integration_ops::inbox_message_write_back` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::error_task_detail` | `web_api::core::handler::integration_ops::error_task_detail` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::error_task_create` | `web_api::core::handler::integration_ops::error_task_create` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::integration_task_action` | `web_api::core::handler::integration_ops::integration_task_action` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::integration_task_completion` | `web_api::core::handler::integration_ops::integration_task_completion` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::difference_detail` | `web_api::core::handler::integration_ops::difference_detail` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::difference_create` | `web_api::core::handler::integration_ops::difference_create` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::integration_ops::difference_decision` | `web_api::core::handler::integration_ops::difference_decision` | wiring | `583d34ca23eb1477` |
| `web_api::core::handler::supplier_api::supplier_api_connection_command` | `web_api::core::handler::supplier_api::supplier_api_connection_command` | wiring | `214c09be8ee6f2af` |
| `entities::integration_ops::w29_work_items::new_error_work_item` | `erp_processes::integration_resolution::work_item_factory::new_error_work_item` | factory | `604f4388bb0258ea` |
| `entities::integration_ops::w29_work_items::new_difference_work_item` | `erp_processes::integration_resolution::work_item_factory::new_difference_work_item` | factory | `604f4388bb0258ea` |
| `services::integration_ops::error_task::impl<IntegrationOpsService>::create_error_task` | `erp_processes::integration_resolution::error_task::impl<IntegrationResolutionProcess>::create_error_task` | creation, read | `641cf66253be297d` |
| `services::integration_ops::error_task::impl<IntegrationOpsService>::error_task_detail` | `erp_read_models::integration_center::error_task::impl<IntegrationCenterReadService>::error_task_detail` | creation, read | `f5945fc689cef6a1` |
| `services::integration_ops::error_task::impl<IntegrationOpsService>::store_error_task` | `erp_processes::integration_resolution::error_task::impl<IntegrationResolutionProcess>::store_error_task` | creation, read | `641cf66253be297d` |
| `services::integration_ops::evidence::impl<IntegrationEvidenceAuthority for Database>::query_original` | `erp_processes::integration_resolution::evidence_adapter::impl<IntegrationEvidenceAuthority for MongoIntegrationEvidenceAuthority>::query_original` | evidence | `6d60a7d5186935aa` |
| `services::integration_ops::evidence::impl<IntegrationEvidenceAuthority for Database>::replay_original` | `erp_processes::integration_resolution::evidence_adapter::impl<IntegrationEvidenceAuthority for MongoIntegrationEvidenceAuthority>::replay_original` | evidence | `6d60a7d5186935aa` |
| `services::integration_ops::evidence::impl<IntegrationEvidenceAuthority for Database>::verify_evidence` | `erp_processes::integration_resolution::evidence_adapter::impl<IntegrationEvidenceAuthority for MongoIntegrationEvidenceAuthority>::verify_evidence` | evidence | `6d60a7d5186935aa` |
| `services::integration_ops::evidence::impl<IntegrationEvidenceAuthority for Database>::discover_evidence` | `erp_processes::integration_resolution::evidence_adapter::impl<IntegrationEvidenceAuthority for MongoIntegrationEvidenceAuthority>::discover_evidence` | evidence | `6d60a7d5186935aa` |
| `services::integration_ops::inbox_message::impl<IntegrationOpsService>::register_inbox_message` | `erp_processes::integration_resolution::inbox_message::impl<IntegrationResolutionProcess>::register_inbox_message` | register, creation | `6f9050405c50bc11` |
| `services::integration_ops::inbox_message::impl<IntegrationOpsService>::write_back_inbox_result` | `erp_processes::integration_resolution::inbox_message::impl<IntegrationResolutionProcess>::write_back_inbox_result` | register, creation | `6f9050405c50bc11` |
| `services::integration_ops::reconciliation_difference::impl<IntegrationOpsService>::create_difference` | `erp_processes::integration_resolution::reconciliation_difference::impl<IntegrationResolutionProcess>::create_difference` | creation, read | `2808ce65b2fd3b46` |
| `services::integration_ops::reconciliation_difference::impl<IntegrationOpsService>::difference_detail` | `erp_read_models::integration_center::reconciliation_difference::impl<IntegrationCenterReadService>::difference_detail` | creation, read | `3991a18b220af63c` |
| `services::integration_ops::reconciliation_difference::impl<IntegrationOpsService>::store_difference` | `erp_processes::integration_resolution::reconciliation_difference::impl<IntegrationResolutionProcess>::store_difference` | creation, read | `2808ce65b2fd3b46` |
| `services::integration_ops::task_decision::action::impl<IntegrationOpsService>::apply_task_action` | `erp_processes::integration_resolution::task_decision::action::impl<IntegrationResolutionProcess>::apply_task_action` | receipt, task, evidence | `c8045d15561b168c` |
| `services::integration_ops::task_decision::action::impl<IntegrationOpsService>::transact_task_action` | `erp_processes::integration_resolution::task_decision::action::impl<IntegrationResolutionProcess>::transact_task_action` | receipt, task, evidence | `c8045d15561b168c` |
| `services::integration_ops::task_decision::action::impl<IntegrationOpsService>::recover_task_action` | `erp_processes::integration_resolution::task_decision::execution::execute_with_receipt` | receipt, task, evidence | `2552a09db3e1b486` |
| `services::integration_ops::task_decision::action::execute_task_action` | `erp_integration::service::task_decision::action::execute_task_action` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::action::execute_error_task_action` | `erp_integration::service::task_decision::action::execute_error_task_action` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::action::error_action_fact` | `erp_integration::service::task_decision::action::error_action_fact` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::action::query_action_fact` | `erp_integration::service::task_decision::action::query_action_fact` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::action::execute_difference_task_action` | `erp_integration::service::task_decision::action::execute_difference_task_action` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::action::difference_action_fact` | `erp_integration::service::task_decision::action::difference_action_fact` | receipt, task, evidence | `e9d55bcd45da75ed` |
| `services::integration_ops::task_decision::complete::impl<IntegrationOpsService>::complete_task` | `erp_processes::integration_resolution::task_decision::complete::impl<IntegrationResolutionProcess>::complete_task` | receipt, task, evidence | `d0b22b0dc4fe3953` |
| `services::integration_ops::task_decision::complete::impl<IntegrationOpsService>::transact_task_completion` | `erp_processes::integration_resolution::task_decision::complete::impl<IntegrationResolutionProcess>::transact_task_completion` | receipt, task, evidence | `d0b22b0dc4fe3953` |
| `services::integration_ops::task_decision::complete::impl<IntegrationOpsService>::recover_task_completion` | `erp_processes::integration_resolution::task_decision::execution::execute_with_receipt` | receipt, task, evidence | `2552a09db3e1b486` |
| `services::integration_ops::task_decision::complete::complete_domain_item` | `erp_integration::service::task_decision::complete::complete_domain_item` | receipt, task, evidence | `9b37fc9c31c4dc63` |
| `services::integration_ops::task_decision::complete::complete_error_task` | `erp_integration::service::task_decision::complete::complete_error_task` | receipt, task, evidence | `9b37fc9c31c4dc63` |
| `services::integration_ops::task_decision::complete::complete_difference` | `erp_integration::service::task_decision::complete::complete_difference` | receipt, task, evidence | `9b37fc9c31c4dc63` |
| `services::integration_ops::task_decision::direct::impl<IntegrationOpsService>::decide_difference` | `erp_processes::integration_resolution::task_decision::direct::impl<IntegrationResolutionProcess>::decide_difference` | receipt, task, evidence | `6fe675e318176b25` |
| `services::integration_ops::task_decision::direct::impl<IntegrationOpsService>::transact_direct_decision` | `erp_processes::integration_resolution::task_decision::direct::impl<IntegrationResolutionProcess>::transact_direct_decision` | receipt, task, evidence | `6fe675e318176b25` |
| `services::integration_ops::task_decision::direct::impl<IntegrationOpsService>::recover_direct_decision` | `erp_processes::integration_resolution::task_decision::execution::execute_with_receipt` | receipt, task, evidence | `2552a09db3e1b486` |
| `services::integration_ops::task_decision::direct::direct_decision_fact` | `erp_integration::service::task_decision::direct::direct_decision_fact` | receipt, task, evidence | `c50eeb12962facae` |
| `services::integration_ops::task_decision::guard::impl<IntegrationOpsService>::replay_receipt` | `erp_processes::integration_resolution::task_decision::guard::impl<IntegrationResolutionProcess>::replay_receipt` | receipt, task, evidence | `9e0847ea7df0d66f` |
| `services::integration_ops::task_decision::guard::ensure_work_item_association` | `erp_processes::integration_resolution::task_decision::guard::ensure_work_item_association` | receipt, task, evidence | `9e0847ea7df0d66f` |
| `services::integration_ops::task_decision::guard::load_error_task` | `erp_integration::service::task_decision::guard::load_error_task` | receipt, task, evidence | `e39c312bf446ba60` |
| `services::supplier_api::governance::jobs::impl<SupplierApiService>::process_health_job` | `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::process_health_job` | jobs | `5f1ac538bbba858c` |
| `services::supplier_api::governance::jobs::impl<SupplierApiService>::process_catalog_job` | `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::process_catalog_job` | jobs | `55ae033502a0b416` |
| `services::supplier_api::governance::jobs::persist_health_failure_task` | `erp_processes::supplier_connection_execution::failure::persist_health_failure_task` | jobs | `a53a2a88dcc5ba34` |
| `services::supplier_api::impl<SupplierApiService>::new` | `services::supplier_api::impl<SupplierApiService>::new` | wiring | `87194d49ae432ad9` |

## raw 静态报告逐项对齐

- 输入：`/private/tmp/erp-integration14-contract-sealed/missing-drift-report.json`，SHA256 `9a9a0ec2e0c191c9228974e9b2dfacb98ef5debbeb1433d344dc33aa1c177b55`。
- raw changed/missing/added 均为 0；needs_review 48 项由 46 条其他解析局限和 2 组基础源码变化构成。本节只核验本阶段 7 transaction + 6 idempotency 名称。
- 四个连接后台 start/finish 方法仍在 after transaction 清单且 token hash 与 before 相同；它们离开含幂等身份的原 jobs.rs 后不再命中 idempotency 分类，不能据此称为方法删除。
- replay_receipt 保持真实读取入口，decode_receipt 为其提取后的身份和载荷验证 provider；两个 scanner 名称指向同一已核验调用链。

| raw 分类/名称 | 实际 before | 实际 after | 结论 |
| --- | --- | --- | --- |
| idempotency / `fn::decode_receipt` | `services::integration_ops::task_decision::guard::impl<IntegrationOpsService>::replay_receipt` | `erp_processes::integration_resolution::task_decision::guard::impl<IntegrationResolutionProcess>::replay_receipt`<br>`erp_processes::integration_resolution::task_decision::guard::decode_receipt` | 原 replay_receipt 实体仍在；身份/消息/JSON/fingerprint 顺序提取到生产 decode_receipt，scanner 分类命中随函数内容变化；不是回放删除。 |
| idempotency / `fn::finish_catalog_job` | `services::supplier_api::governance::jobs::impl<SupplierApiService>::finish_catalog_job` | `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::finish_catalog_job` | 原函数主体 token 未变，迁出 jobs.rs 后 scanner 不再归 idempotency；after transaction 分类含相同 token_sha256；实际 start/finish provider 与外层网关步骤已核验。 |
| idempotency / `fn::finish_health_job` | `services::supplier_api::governance::jobs::impl<SupplierApiService>::finish_health_job` | `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::finish_health_job` | 原函数主体 token 未变，迁出 jobs.rs 后 scanner 不再归 idempotency；after transaction 分类含相同 token_sha256；实际 start/finish provider 与外层网关步骤已核验。 |
| idempotency / `fn::replay_receipt` | `services::integration_ops::task_decision::guard::impl<IntegrationOpsService>::replay_receipt` | `erp_processes::integration_resolution::task_decision::guard::impl<IntegrationResolutionProcess>::replay_receipt`<br>`erp_processes::integration_resolution::task_decision::guard::decode_receipt` | 原 replay_receipt 实体仍在；身份/消息/JSON/fingerprint 顺序提取到生产 decode_receipt，scanner 分类命中随函数内容变化；不是回放删除。 |
| idempotency / `fn::start_background_job` | `services::supplier_api::governance::jobs::impl<SupplierApiService>::start_background_job` | `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::start_background_job` | 原函数主体 token 未变，迁出 jobs.rs 后 scanner 不再归 idempotency；after transaction 分类含相同 token_sha256；实际 start/finish provider 与外层网关步骤已核验。 |
| idempotency / `fn::start_health_job` | `services::supplier_api::governance::jobs::impl<SupplierApiService>::start_health_job` | `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::start_health_job` | 原函数主体 token 未变，迁出 jobs.rs 后 scanner 不再归 idempotency；after transaction 分类含相同 token_sha256；实际 start/finish provider 与外层网关步骤已核验。 |
| transaction / `fn::register_inbox_message` | `services::integration_ops::inbox_message::impl<IntegrationOpsService>::register_inbox_message` | `erp_processes::integration_resolution::inbox_message::impl<IntegrationResolutionProcess>::register_inbox_message` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::store_difference` | `services::integration_ops::reconciliation_difference::impl<IntegrationOpsService>::store_difference` | `erp_processes::integration_resolution::reconciliation_difference::impl<IntegrationResolutionProcess>::store_difference` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::store_error_task` | `services::integration_ops::error_task::impl<IntegrationOpsService>::store_error_task` | `erp_processes::integration_resolution::error_task::impl<IntegrationResolutionProcess>::store_error_task` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::transact_direct_decision` | `services::integration_ops::task_decision::direct::impl<IntegrationOpsService>::transact_direct_decision` | `erp_processes::integration_resolution::task_decision::direct::impl<IntegrationResolutionProcess>::transact_direct_decision` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::transact_task_action` | `services::integration_ops::task_decision::action::impl<IntegrationOpsService>::transact_task_action` | `erp_processes::integration_resolution::task_decision::action::impl<IntegrationResolutionProcess>::transact_task_action` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::transact_task_completion` | `services::integration_ops::task_decision::complete::impl<IntegrationOpsService>::transact_task_completion` | `erp_processes::integration_resolution::task_decision::complete::impl<IntegrationResolutionProcess>::transact_task_completion` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |
| transaction / `fn::write_back_inbox_result` | `services::integration_ops::inbox_message::impl<IntegrationOpsService>::write_back_inbox_result` | `erp_processes::integration_resolution::inbox_message::impl<IntegrationResolutionProcess>::write_back_inbox_result` | 根事务仍在新实际 Process；提取的领域准备/写入或任务步骤 provider 已逐 Executor、顺序、时钟、ID、权限和错误核验。 |

## 关键仓储、错误与回执形状

- FailedMessage 的真实 IntegrationOpsRepository::create_error_task_with_message_failure 与 InboxMessageRepository::update 均与输入主体规范化一致；后者继续进入未变的通用 Repository::update/CAS，原 Executor 直达 mongo_ops。
- 本域错误到 services 的转换逐 variant 保留；本域唯一索引在原通用映射中均为默认重复提示，仍为 ConflictError。原乐观锁、TransientTransaction、OutcomeUnknown、Logic 和 HTTP 入口映射保持。
- ReceiptEnvelope、ActionReceiptMessage、CompletionReceiptMessage、DirectReceiptMessage 的全部字段与 serde 属性 token 相同，独立形状 SHA256 及实际定义位置在 JSON receipt_shapes。

## 未变基础与记录封存

以下文件逐字节比较一致；其完整 SHA256 载于 JSON。
- `crates/persistence-core/src/transaction.rs`：`3fa042ed3d5baaa9914f55566fd961c40d58ebcd9a7f58f0e67497bdabdb47ed`。
- `crates/persistence-core/src/executor.rs`：`d7f18681920bb0f7356687953043ea004f2c96b0a3219f4cb8a46242a7303a2d`。
- `crates/persistence-core/src/errors.rs`：`5ae8a1312fa5eb408eccb36c59d6021ed143385d8c27bdfbddd6a705362e6b50`。
- `crates/persistence-core/src/repository/base.rs`：`466d0bb968a321422e288e5355ca4814b5fe6e1e801409ea3408824db64b715c`。
- `crates/persistence-core/src/mongo_ops.rs`：`4ca2aed90046ad3a0957637f4085856626d30c9b5264691479eaae737b54ee6d`。
- `crates/application-core/src/command.rs`：`606633a4c45560ad8b8994aa6f6d933a4264e1558b6f7ec2111b8e5837adfa73`。
- `crates/erp-workflow/src/service/work_item/close.rs`：`6dcd40da56df8f196f81a55aad423ec500e7cfb387a98f988d45a1eee4664503`。
- `crates/erp-workflow/src/service/work_item/access.rs`：`1a9abc8ada76c6ce7c1ecb5a53797cd3af5ae77fc4b10408ae49a568a3952d7c`。
- `apps/web-api/src/core/routes/integration_ops.rs`：`c45e14866d2d247eeb11f4f1e97b702e169bde46e076d3948e4cda0ec336f153`。
- `apps/web-api/src/core/routes/supplier_api.rs`：`0c0d828c59579dde112f2d059893c080c79ded383310295c4d96036f7ed48c16`。

- after_files 的全部文件在生成记录前再次核对 SHA256；任何并发修改使生成器失败，必须刷新并复核后重绑。
- 真实数据库事务、提交不确定恢复、并发唯一性和外部网关行为均未实跑；本源码核验不得替代该级别证据。
