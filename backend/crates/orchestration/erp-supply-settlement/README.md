# erp-supply-settlement

## 职责

`SupplierSettlementProcess<A>` 是供应商结算（D33）的跨域命令入口：登记来源证据、创建或刷新试算、处理对账责任与差异、提交财务复核，并在正式复核确认时生成结算应付。提交复核只冻结主题和派发待办，不等于确认、付款或核销。

- [供给域 `erp-supply`](../../domains/erp-supply/src/service/supplier_settlement/mod.rs) 持有结算单、冻结明细、差异、补证和来源批次，负责试算、不变式、状态迁移和仓储。正式履约、退款及退款分配是结算的上游事实；供应商流水是待核验的证据，不能替代这些事实。
- [工作流域 `erp-workflow`](../../domains/erp-workflow/README.md) 持有工作项及其资格、责任和状态规则；复核决定由结算领域保存。[财务域 `erp-finance`](../../domains/erp-finance/README.md) 持有应付子账、原始分录和成本事实；本 crate 传入结算事实并协调写入，不重写财务规则。
- [供应商结算读模型](../../read-models/erp-read-models/src/supplier_center/settlement/mod.rs) 负责结算单列表、跨域详情与展示投影；供给域提供明细、差异和来源概要等单域读取。本 crate 只保留命令相关的候选查询和访问检查，不以页面按钮或投影替代写入授权。

## 目录结构

| 源码入口 | 用途 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`src/lib.rs`](src/lib.rs)、[`errors.rs`](src/errors.rs) | 声明依赖，公开 `supply_settlement`、`SupplierSettlementProcess` 和 `Error` / `Result`；区分权限拒绝、版本冲突、瞬态事务冲突和提交结果未知。 |
| [`supply_settlement/mod.rs`](src/supply_settlement/mod.rs)、[`source.rs`](src/supply_settlement/source.rs)、[`draft.rs`](src/supply_settlement/draft.rs)、[`void.rs`](src/supply_settlement/void.rs) | 装配 Process，登记来源批次，创建、刷新和作废结算草稿。 |
| [`evidence.rs`](src/supply_settlement/evidence.rs)、[`difference.rs`](src/supply_settlement/difference.rs)、[`handover.rs`](src/supply_settlement/handover.rs) | 追加差异补证、登记正式差异结论、交接对账负责人及改派差异处理人。 |
| [`review.rs`](src/supply_settlement/review.rs)、[`reviewers.rs`](src/supply_settlement/reviewers.rs) | 筛选和复验财务复核人，提交冻结主题并创建正式待办，办理确认或驳回及原结果回放。 |
| [`review_preparation.rs`](src/supply_settlement/review_preparation.rs)、[`review_posting.rs`](src/supply_settlement/review_posting.rs) | 先准备财务结果和复核决定，再按固定顺序复验并持久化。 |
| [`shared.rs`](src/supply_settlement/shared.rs)、[`list.rs`](src/supply_settlement/list.rs) | 派生稳定命令身份；`require_detail` 只检查指定单据的详情访问资格，不生成列表或详情。 |

生产仓储归所属领域；本地 [`repository/`](src/supply_settlement/repository/mod.rs) 仅在 `cfg(test)` 下编译。

## 架构设计

### 授权、装配与事务

[服务端组合根](../../../apps/erp-server/src/wiring/settlement_data_scope.rs) 为 Process 注入数据库、结算范围访问器 [`SettlementAccess`](../../domains/erp-supply/src/service/supplier_settlement/access.rs)、共享 `SharedRbacService`、`AuthorizationService` 和 `WorkItemService<A>`。HTTP 传递认证所得的 `AuditActor`；用例按真实对象核对功能权限、对账负责人、差异处理人及组织范围，复核则按正式任务核对当前指派与办理资格。见 [业务授权 API](../../domains/erp-identity/README.md#业务授权-api)。身份策略变更及其授权版本复验仍由 [identity 策略事务](../../domains/erp-identity/src/service/iam/rbac/transaction.rs) 负责，结算事务不更新 RBAC 策略。

Process 为每条跨域写命令开启事务。决定写入的授权、事实读取与复验、领域仓储、工作项、财务写入、适用的回执及成功审计复用同一 `Executor`；事务内步骤不另开事务，也不调用供应商 HTTP 或对象存储。独立读取遵循[读取边界](../../foundation/persistence-core/README.md#读取边界)。

### 实际链路：编制、核对、审批与过账

1. **编制：登记来源，再冻结试算。** `record_source_evidence` 校验录入权限，由[供给域来源服务](../../domains/erp-supply/src/service/supplier_settlement/source.rs)核对期间、策略与版本、供应商和履约行归属，并确认批次未漏掉期间内可枚举的正式履约完成或退款明细。订单与退款的含税、不含税及税额从 D32 的履约成本快照、正式退款事实和分配派生；运费、服务费和供应商账单行可随证据提交，取消时间须附正式引用且订单确已取消。若引用已采集的 API 流水，先[关联原流水](../erp-supplier-orders/src/supplier_after_sales/settlement_source.rs)，再写不可变来源批次和审计。

   创建草稿从服务端读取同供应商、同期间的最新完整批次，原子写入单头、明细、差异和审计，不接受客户端逐行指定试算金额。刷新沿用冻结的期间策略，核对单据版本与旧来源摘要；来源不变只登记回执和审计，不提升版本，来源变更则[物理替换旧明细、差异及其补证](../../domains/erp-supply/src/repository/supplier_settlement/command/draft_write.rs)，并更新来源和主题摘要。
2. **核对：补证与正式结论分开。** `append_difference_evidence` 在功能资格和本单范围内追加不可变证据引用及意见，不直接改差异结论；[本域补证步骤](../../domains/erp-supply/src/service/supplier_settlement/evidence_posting.rs)在追加前更新结算单版本，避免迟到补证穿过提交复核的并发校验。`decide_difference` 还要求实际差异处理人资格，核对归属、结算单与差异版本，正式结论只能引用该差异已登记的证据；[领域服务](../../domains/erp-supply/src/service/supplier_settlement/difference.rs)重算主题摘要并依次以乐观锁（CAS）写结算单、差异，随后写回执和审计。对账负责人交接不改派已打开的财务待办，差异处理人改派不改变对账负责人。未提交复核的可编辑草稿可经授权和版本校验作废。
3. **审批：冻结主题并办理财务复核。** `submit_review` 核对经办资格、单据版本、主题摘要、刷新截止策略 ID 与版本；写入前重读冻结明细和正式差异，拒绝未解决差异或摘要不符，并复验所选复核人。结算单进入 `PENDING_REVIEW` 后不可编辑；同一事务创建绑定单据与主题摘要的 `SUPPLIER_SETTLEMENT_REVIEW` 工作项，指派给具备固定财务资格、不是本单制单人的账号，责任角色为 `role-finance`，责任组织为单据的有效内部组织（不能是公司根）。提交回执和成功审计随之写入。`decide_review` 的确认、驳回都核对单据版本、开放任务及其版本、冻结主题、当前责任与资格，并遵守制单与复核分离；驳回须有结构化原因，完成待办并退回 `DRAFT` 或 `HAS_DIFFERENCE`，不生成财务分录。
4. **过账：确认与财务结果一起提交。** 确认需具备完整供应商账单身份；供给域按冻结明细和 ERP 接受的正式差异计算含税、不含税、税额差额。[财务应付步骤](../../domains/erp-finance/src/service/payable/supplier_settlement.rs)按 ERP 试算含税金额加接受的含税差额构造来源为 `SupplierSettlement` 的应付子账和原始增加分录，以结算单号、主题摘要追溯来源。[财务成本步骤](../../domains/erp-finance/src/service/cost/supplier_settlement.rs)目前只允许全零差额（不写成本分录）；任一分量非零时因缺少权威原成本、税率及费用分摊链而阻断，在改变结算状态或完成任务前返回业务错误，不伪造 `CostEntry`。财务准备成功后，`review_posting` 沿原事务复验授权、岗位分离、明细、差异和主题，依次写入结算单 → 工作项 → 应付子账及原始分录（如适用）→ 成本分录（如适用）→ 决定回执 → 成功审计。任一步失败即停止，不提交部分结果。

### 幂等、查证与审计

各命令保留自己的回放条件。创建按确定性 `statement_no` 查重并比对供应商和期间；来源登记、差异补证按原请求 ID 和载荷指纹回读领域记录；已作废单据仍须通过原作废授权才返回当前状态。刷新、差异结论、责任变更、提交复核和复核决定使用[供给域独立命令回执](../../domains/erp-supply/src/command_receipt/mod.rs)，核对原操作人、动作、对象、幂等键和请求指纹，并按各命令要求复验当前授权及结果。例如，刷新要求版本和来源摘要仍匹配，复核决定要求结算单、已完成任务及应付结果仍与原回执一致；提交复核则允许单据和任务在原提交后推进，只要任务仍绑定该单和冻结主题。

成功审计沿用 [`persist_log`](../../../docs/audit-logging.md#统一执行入口)，用于追溯操作人与对象，不作为复核结果、财务事实或回执的来源；回放不重写业务、不重复记录成功审计。写命令需要恢复时，只查证原命令，不重跑写入。采用 [`recover_command`](../../domains/erp-audit/src/execution/recovery.rs) 的入口在查无原结果时保留首次错误，提交结果未知且查证失败时也保留首次未知错误；重试沿用原命令身份和载荷。

### 相邻编排分工

- [`erp-supplier-orders`](../erp-supplier-orders/README.md) 负责供应商 API 订单、售后及[分页采集对账流水](../erp-supplier-orders/src/supplier_after_sales/settlement.rs)。本 crate 只单向复用来源关联步骤：核对既有流水与原订单、连接及正式退款事实后关联到本批次，不在结算事务内采集流水或发起退款。
- [`erp-fulfillment-execution`](../erp-fulfillment-execution/README.md) 负责履约执行命令；本 crate 只单向复用其[供应链回执持久化步骤](../erp-fulfillment-execution/src/supply_execution/receipt.rs)，沿用结算事务和供给域回执，不调用履约写命令。
- [`erp-finance-posting`](../erp-finance-posting/README.md) 编排付款、核销、发票等独立财务用例；结算确认直接复用财务域的应付和成本步骤，不另起财务 Process。[`erp-approval-runtime`](../erp-approval-runtime/README.md) 提供通用审批实例运行步骤；结算走固定的财务复核工作项，不依赖它启动通用审批实例。

跨编排依赖遵循[依赖与接入要求](../../README.md#依赖与接入要求)，不引入应用装配、具体协议适配器或旧 `erp-processes`。
