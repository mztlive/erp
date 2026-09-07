# 阶段 11 幂等与事务原始告警核销

## 1. 核销身份与执行边界

- Before：`3f5e8c0daefc5a234f30e496b850cbcbb7a7e8ba`，工作树 `/private/tmp/erp-domain-crate-10-sales`。
- After：源码提交固定为 `0a297ab00d29b28047e05f780b4815c0a5456a2f`，工作树 `/private/tmp/erp-domain-crate-11-procurement`；41 份逐文件 SHA-256 已与 sealed inventory 及该源码提交逐一复核。
- 原始输入：`/private/tmp/erp-procurement11-contract-sealed/missing-drift-report.json`。changed/missing/added 均为 0，原始状态仍为 `needs_review`；本证据不得改写该扫描状态。
- 核销范围固定为 18 项 idempotency 和 7 项 transaction，共 25 个分类符号条目；其余 46 个词法解析项单独登记于 `/private/tmp/procurement11-parser-limitations-review.json`。
- 结论级别固定为源码复核。未执行 Cargo、历史 `tests/**`、HTTP 或数据库操作；真实数据库运行未验证。

## 2. 同名符号与词法漏选处理

- raw `fn::apply` 指定 `start_approval.rs:509` 的 `StartPosting::apply`；不得误指 `MongoPosting::apply`、`CancelPosting::apply`、`VoidPosting::apply`、`MongoEffect::apply` 或测试 `RecordingSteps::apply`。上述生产分发分别展开核验，测试实现不作为实际写入证据。
- raw `fn::receipt` 指定 `void_order.rs:293` 的 `VoidPosting::receipt`；trait 声明和测试替身分开处理。
- raw `fn::persist_with_port` 指定 `sourcing_create/stock_posting.rs:97` 的真实库存写段；`persist_stock_allocations` 使用 `StockAllocationAdapter` 调用该段并原样传入根 session。
- `persist_purchase_order_start_with_session` 和 `void_order_and_persist` 在 After 仍存在并被生产入口调用。拆分后 wrapper 不再包含原扫描关键词，原始 After 选集中为空；不得登记为业务函数或合同被删除。
- raw `fn::submit` 指定采购订单 process 的独立提交方法；采购提交实体及其他业务同名状态方法不混入该条目。

## 3. 逐原始符号登记

| 分类 | 原始符号 | 核销组 | 结果 |
| --- | --- | --- | --- |
| idempotency | `fn::apply` | start | 源码核销，无可观察漂移 |
| idempotency | `fn::create_from_basis_in_transaction` | basis | 源码核销，无可观察漂移 |
| idempotency | `fn::create_from_sourcing_in_transaction` | sourcing | 源码核销，无可观察漂移 |
| idempotency | `fn::execute_save_draft_transaction` | save_root | 源码核销，无可观察漂移 |
| idempotency | `fn::execute_void_draft_transaction` | void_root | 源码核销，无可观察漂移 |
| idempotency | `fn::freeze_change_submission` | change_freeze | 源码核销，无可观察漂移 |
| idempotency | `fn::persist_draft_replacement` | save_write | 源码核销，无可观察漂移 |
| idempotency | `fn::persist_purchase_order_start_with_session` | start | 源码核销，无可观察漂移 |
| idempotency | `fn::persist_stock_allocations` | stock | 源码核销，无可观察漂移 |
| idempotency | `fn::persist_with_port` | stock | 源码核销，无可观察漂移 |
| idempotency | `fn::receipt` | void_write | 源码核销，无可观察漂移 |
| idempotency | `fn::replay_creation` | receipt_creation | 源码核销，无可观察漂移 |
| idempotency | `fn::replay_purchase_submit` | submit | 源码核销，无可观察漂移 |
| idempotency | `fn::replay_saved_draft` | receipt_save | 源码核销，无可观察漂移 |
| idempotency | `fn::replay_sourcing` | receipt_sourcing | 源码核销，无可观察漂移 |
| idempotency | `fn::replay_void_draft` | receipt_void | 源码核销，无可观察漂移 |
| idempotency | `fn::submit` | submit | 源码核销，无可观察漂移 |
| idempotency | `fn::void_order_and_persist` | void_write | 源码核销，无可观察漂移 |
| transaction | `fn::apply` | start | 源码核销，无可观察漂移 |
| transaction | `fn::freeze_change_submission` | change_freeze | 源码核销，无可观察漂移 |
| transaction | `fn::persist_purchase_order_cancel` | cancel | 源码核销，无可观察漂移 |
| transaction | `fn::persist_purchase_order_start_with_session` | start | 源码核销，无可观察漂移 |
| transaction | `fn::replay_purchase_submit` | submit | 源码核销，无可观察漂移 |
| transaction | `fn::submit` | submit | 源码核销，无可观察漂移 |
| transaction | `fn::write_effective_change` | change_effect | 源码核销，无可观察漂移 |

## 4. 生产路径核销合同

### start：启动原事务写段与 StartPosting::apply

原函数仍由独立提交和创建后提交两种入口调用；After wrapper 本体不再包含收据关键词而未入词法选集，实际生产写段迁到 StartPosting::apply；该 apply 与其他流程同名方法及测试替身分别识别。

- PreparedExecution 非 Apply 立即返回 None，不产生写入。
- Receipt 首写 → 正式号 DocumentGuard → 可选销售 procurement guard 与覆盖重验 → 冻结提交/行/订单 → superseded draft → runtime/快照/开放任务 → 补 first_task 的业务审计回执。
- 所有步骤使用传入 Executor；独立提交只由外层 with_transaction 开事务，创建后提交复用创建根 session。
- 原缺入口执行错误、采购启动守卫冲突、回执首写映射和首次任务返回保持。

### basis：创建依据命令串行化与提交

来源销售完整实体只在组合层持有，find_requested_group/basis_id_for 改传原 base.id 与 guard 的窄事实；域规则和实际 provider 逐层展开核验。

- 事务前授权快照后回放；根 run_authorized_policy_transaction 内先校验账号并再次回放。
- 原开放本人任务及冻结责任范围 → 已生效销售 → 初次依据/范围 → 销售 guard CAS → 再读依据和供给事实 → 数量校验 → 创建。
- 目标仓库和初始履约负责人资格先于 basis ID、采购 ID 和实体构造；供应商商务指针和付款解析先于提交 ID；各行 ID 按原行序。
- 绑定、采购单、BusinessDocument、草稿头和行、任务同步后，以同 session 冻结提交和启动审批；最终创建回执保存正式号及提交后 version。
- 任意事务错误只回读一次相同创建收据；未命中保留原错误。

### sourcing：选源创建、库存与采购分批顺序

SourcingPlan 和 basis_id_for 的销售输入替换为无副作用窄事实投影；库存写入函数抽真实 Port；原供给顺序未改变。

- 事务内回放 → 本人开放采购任务 → 生效销售 → 采购和库存初次依据 → plan → 单次销售 guard CAS。
- 最新库存依据重验 → 顺序库存预占/分录 → 按仓归组仓发草稿 → 最新采购供给依据重验。
- 按原 purchase_plans 顺序构造每单 basis ID 与原 sourcing-item 收据身份，再复用 persist_basis_draft 同 session 建单并提交。
- 全部采购完成后同步任务、读取同步后 task status、写整批 sourcing 收据；库存/订单响应行序与参考身份保持。
- 原授权策略事务、账号重验、任意错误后一次同收据回放保持。

### save_root：保存草稿授权事务与错误恢复

旧 Service 接收者改为 Process；事务函数体按路径归一后相同，采购纯校验/仓储移领域，副作用仍由根编排。

- req validate/shape → 原授权 → 排除幂等键的指纹与收据身份 → 事务外回放。
- policy revision 授权事务内账号校验 → 事务内回放 → 采购创建人 → version → status → 付款条件不得改变。
- 读取旧草稿 → 销售 guard/覆盖 → 全量请求行编辑校验 → 金额和新草稿头/行 ID → 原订单 touch、旧草稿 supersede、当前指针。
- 任何事务错误只作一次 NoTransaction 相同收据回放；没有回执保留原错误。

### save_write：保存草稿领域持久化拆分

前四个采购写操作抽入 persist_replacement；展开后与原 persist_draft_replacement 写段等价。

- 旧草稿 CAS → 新草稿头 → 按原行序逐行创建 → 采购单 CAS → 采购任务同步。
- Repository 更新后的 version 与新草稿三金额形成原 SaveDraftReceipt。
- 相同 action/resource/receipt_id/fingerprint 编码审计回执；审计末写；失败停止，不另开事务。
- 新草稿 ID、DRAFT 后缀 ID、逐行 ID 与原金额校验位置保持。

### void_root：作废授权事务与错误恢复

旧 Service 接收者改为 Process；授权根函数体相同，状态约束和采购 CAS 移领域。

- validate → 原授权 → 指纹/身份 → 外部回放；policy 授权事务内再次验证账号与回放。
- 创建人优先；已 Voided 且无匹配回执先返回冲突，再比较 version 和 Draft；当前提交必须仍 Draft。
- 任意事务失败后只回放一次；只有稳定回执命中且当前单据 Voided、version 不倒退才返回 replayed=true。

### void_write：作废真实步骤及 VoidPosting::receipt

void_order_and_persist 仍是生产入口但 wrapper 不含扫描关键词；原写段拆成 VoidPosting::apply 和 VoidPosting::receipt；receipt 是具体生产实现，不是 trait 声明或测试替身。

- 销售单读取/guard CAS → 采购 transition(Voided)/CAS → 任务同步 → 从更新后 version 与规范化 reason 形成收据。
- receipt_id/action/resource_type/resource_id/fingerprint 与 encode_message 保持；最终 audit.create 后返回 replayed=false。
- 相同 executor 贯穿三个步骤和 receipt；任何步骤失败不调用后续 receipt。

### receipt_creation：创建回放与审计窄事实

decode 输入改为六个原字段的 AuditReceiptFact；函数体除该无副作用投影外一致。

- 收据 audit 不存在返回 None；身份→消息形态→指纹→payload 解码顺序保持。
- IdentityMismatch/PayloadConflict 同映射原 Conflict 文案；Corrupted 仍 Internal。
- audit.resource_id 与payload采购ID一致 → 读取该采购单 → 主键一致；回放响应不重取订单号或version。

### receipt_save：保存回放与版本下界

只替换 decode 的事实输入并使用迁后单域 load_purchase_order；字段投影和 typed 错误已展开核对。

- 同收据身份/指纹/解码规则；payload.purchase_order_id 必须匹配路径。
- 当前采购 version 不得小于回执 lock_version；返回原回执金额和reference，不以当前行重算。
- 事务前、事务内和失败后均复用同一函数与指定执行器。

### receipt_void：作废回放与状态确认

只替换 decode 的原字段事实输入；当前采购读取和状态约束保持。

- 身份或载荷不一致仍原 Conflict；形态损坏 Internal。
- payload采购ID匹配路径，当前状态必须Voided且version不小于回执。
- 只有全部校验通过返回原回执并 replayed=true。

### receipt_sourcing：选源回放与历史任务状态兼容

decode 的完整审计实体改六字段事实；其余函数体和新旧任务状态兼容函数相同。

- identity target 固定sales_order_id，原sourcing action和指纹。
- 新回执使用冻结 work_item_status；旧回执缺失时读取同工作项并验证类型/业务类型/销售ID，历史任务缺失回退Completed。
- 旧Closed→Completed，新Closed→Internal；orders/stock_reservations顺序、replayed和reference保持。

### submit：独立采购提交与有界恢复

submit 指定为 PurchaseOrderProcess::submit，不与采购提交实体的同名状态方法混用；主体除领域接收者无变化。replay 输入改原字段事实。

- 请求及patch校验 → 原fingerprint和WholeStringJoined旧收据候选 → 先回放 → 订单version/status、冻结binding、Draft及行、销售/组织资格。
- 原正式号分配 → 注册行读取 → Instant::now → 注册号 → 旧草稿失效 → 可选补丁行金额和新冻结提交 → domain状态 → 审批snapshot/命令/graph/receipt/prepared → 审计 → 启动事务。
- patch只有非空时才重验付款不变、设置采购guard；原next_submission_no、submission ID、submit时间、行ID顺序保持。
- 启动仅 command_may_have_committed 时最多8次fresh session验证业务/销售/绑定和BPM回执，再读取业务收据；其他错误直接返回，延迟和original_error保持。
- 回放按identity.id_candidates顺序读，新旧ID兼容；身份错误Internal、同键异载荷Conflict、损坏Internal；只校验采购存在，仍返回回执字段；原reference=submission_no回放行为保持。

### stock：现有库存预占实际 Port

persist_stock_allocations 绑定真实 StockAllocationAdapter 并把原 session 原样传给 persist_with_port；该函数承接原写段，生产方法与替身测试分开辨识。

- 原plan和requested_lines顺序 → latest balance组和行存在性 → reserve_quantity CAS；false立即原Conflict。
- CAS成功后才算原 inventory.allocate_existing_stock 指纹（request_fingerprint/balance_id/稳定行/quantity）。
- 随后 reservation ID/实体 → reservation.create → entry ID/实体 → entry.create → 原响应投影。
- 原StockReservation来源ExistingStock、quantity及零consumed/released、None采购/收货关联和entry source_document_id=audit_id保持；同executor、任一错误停止。

### cancel：采购撤回事务步骤

原Apply分支内事务写段抽成 CancelPosting::apply；Replay入口返回和任务关闭构造时点不变。

- 非Apply立即返回且不关闭任务。
- close_all_for_approval_cancellation在原事务前位置执行，使用原actor/reason/now。
- 唯一with_transaction内 claim_and_persist_document_cancel_runtime（原receipt/runtime/CAS/tasks提供方）→采购CAS→审计；共用session、失败停止。

### change_freeze：变更冻结提交的拆分

原数据库跨供应商/主体名称helper迁唯一readmodel；本域行恢复与header构造移领域；函数体按明确路径和receiver归一相同。

- 基准版本 → 空行时原版本行 → 当前供应商名称 → 行/金额/付款代码 → 付款解析 → next_no → submission ID。
- 其后来源销售当前版本行 → enrich → line ID → submit(Instant::now) → 原normalized_request.lines的Debug/SipHash指纹。
- 原采购头、基准供应商修订/快照、付款None回退及原idempotency_key保持。

### change_effect：变更生效根事务及实际写序

根with_transaction函数体相同，参数由混合EffectiveChangeWrite拆为各域独立计划EffectiveChangePosting；真实写步骤展开核对。

- 原准备NoTransaction查询与基准校验时点保持；不宣称准备读取已在事务中重验。
- 财务差额零不ID/取时；非零先账户ID与账户校验再分录ID/date/posted_at；负差额原账户错误保持；原成本Vec恒空。
- 事务内审计构造→mark_effective→SalesGuard→PrepareAllocations→Revision→Allocations→CurrentOrder→ProcurementTasks→Payable→Submission→Change→Audit。
- 原entry/account来源、submission pending/匹配、基准当前性、CAS、返回order version与所有首错保持。

## 5. 静态对照证据与限定

- 46 个函数以当前 Before/After 源码独立提取函数体，全部在下列明确归一规则后相等：`/tmp/procurement11-source-body-checks.json`。该集合包括采购提交与恢复、创建与选源根/恢复、保存与作废授权事务/恢复、25 项原告警涉及的未拆分核心及 receipt codec。
- 原始 25 个条目引用的 19 份 Before/After 源文件，实际 SHA-256 与原始捕获全部相同：`/tmp/procurement11-raw-source-fingerprints.json`。
- 拆分步骤未使用“同名 hash 相同”代替核验：启动、保存、作废、撤回、库存、变更生效均展开实际生产适配器及其提供方读取/写入调用，按第 4 节记录顺序和首错。
- 辅助材料 `/tmp/procurement11-basis-body-review.json`、`/tmp/procurement11-formalization-semantic-review.md`、`/tmp/procurement11-order-symbols.md` 用于定位；第 4 节关键路径均重新读取当前生产实现。

- 仅移除空白；crate::errors 改 services，entities::purchase_order 改 erp_procurement::entity::purchase_order。
- 原旧service方法迁领域后的 self.domain(). 接收者解引用；不移除函数调用或判断。
- AuditLog→AuditReceiptFact 显式投影六原字段；decode实现逐函数另比。
- sales_order_basis_fact 仅复制原ID/版本guard等字段且无ID/时钟/I/O；对应无副作用事实调用归一。
- workflow_compose、fulfillment公开出口指向同一原函数；supplier_names迁单一readmodel函数。
- 仅归一find_requested_group调用的尾部逗号；不统一tuple语法。

## 6. 登记结论

- 当前核销范围全部完成源码复核，未发现幂等身份、回执字段/历史兼容、ID/取时、前置首错、原写序或传入 Executor 漂移。
- 每个 raw 分类符号的原始来源、实际迁后来源、处理方式和保持合同逐项登记于 `/tmp/procurement11-idempotency-transaction-review.json`。
- 本证据只核销源码差异；根集成人必须独立引用实际门禁与测试日志，不得据此声称真实 MongoDB 回滚、并发、恢复或 HTTP 运行通过。
