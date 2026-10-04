# 审计业务依赖清单与解耦实施合同

本合同规定审计业务依赖的完整静态清单、替代事实来源、代码切换步骤及自动审计接入要求。实施目标是：业务执行、命令回放和责任判断不再反查展示性审计；业务代码声明动作与必要事实，统一机制生成可供业务人员理解的操作记录。

状态：初始静态源码盘点完成；A01—A18、B01—B25、C01—C08、D01—D03 的领域替代及 E 类保留边界已建立代码，待统一验证。§2 的旧调用链只记录改造基线，现代码与验收状态以[逐项实施登记](audit-business-dependency-implementation.md)为准。真实环境运行验收未执行。

## 1 适用范围与基线

1. 盘点完成日期为 2026-10-04。Git HEAD 为 `9c05531092210b45808f1bdc18cca5dd3d37590e`，事实依据包含当时工作区尚未提交的修改，不等同于该提交单独的内容。
2. 生产调用范围为 `backend/apps`、`backend/crates`。补充检查 `erp-client` 的审计展示及结果引用、`scripts` 和 `backend/scripts` 的集合访问。测试、示例、归档及审计历史证据目录不作为运行依赖；不得修改归档以完成本合同。
3. “依赖审计的业务”包括：读取审计恢复命令结果，使用审计字段进行业务校验或参与关系判断，以审计身份生成业务证据或关联来源。仅写入日志仍属于审计一致性要求，但不等同于反向业务依赖。
4. 下文 A、B 按命令或读取实现归组，不按 HTTP 路由数量计数；共享实现涵盖的多个动作必须全部迁移。C、D 可以与 A、B 指向同一业务，不得相加为业务总数。
5. §1.1、§1.2、§2 的方法名、行号及旧读取链定位初始静态基线；链接用于定位对应文件，不能证明旧方法仍存在。已删除旧接口不得重新接入业务；当前领域集合和入口以实施登记及现源码为准。文件移动不得成为遗漏条目的理由。
6. 初次静态盘点只读取源码并编写文档；当时未启动服务，未连接 MongoDB/S3，未执行清库、迁移、E2E、编译或单元测试。后续代码及库单测证据按实施登记记录，静态可达调用链不证明真实环境的事务回滚或并发正确性。

### 1.0 本次未上线实施范围

1. 用户于 2026-10-04 明确：系统尚未上线，本次不需要历史数据及旧版本兼容。A、B、C、D 的业务依赖直接切换到领域回执和正式业务事实，自动审计接入统一执行边界。
2. 本次不实施旧日志双写、旧候选别名、旧协议兼容 parser、历史回填、滚动 writer 共存或旧二进制回退窗口。下文盘点中的旧 ID、指纹、消息及结果是初始基线源码事实；不得把这些事实当作现代码行为或本次需要交付的历史兼容机制。
3. 新回执须以稳定命令身份和唯一索引保护同键同载荷回放、同键异载荷冲突及并发提交。新业务事实、回执和成功审计在同一 Executor 中写入。停止依赖 `uk_audit_logs_id` 防止业务重复，不得以先查后写代替新回执唯一约束。
4. 创建人直接由拥有领域的不可变 `created_by` 提供；缺失或非法身份明确失败，不授予参与资格。供应商付款新增认证创建人字段。两套资金投影同时切换，不保留审计身份回退。
5. C07、C08 直接保存并读取结构化撤回/取消事实；保留当前命令的 actor、实例、执行、原因和终态交叉验证。D01—D03 使用稳定命令身份形成证据、分录来源和结论，不再从展示审计派生。
6. 原入口的结果形态、授权与错误顺序、金额数量、附件归一化、版本、业务唯一约束、任务和外部生命周期仍必须保持。新回执独立保存当前协议，不解析审计 `message`。
7. 本次执行顺序由 §5.1 规定；§2 的旧协议只作基线盘点，全部替代要求执行当前结构化领域事实合同。V03 的旧版本重试及 V12 的历史回填/旧二进制回退不适用。V05、真实事务回滚及最终中文浏览器体验仍须取得独立运行证据，未执行时保持未验证。
8. 不执行清库、真实数据写入、服务启动或集成验收。业务代码和内联库单元测试按仓库质量门禁交付，生产旧协议 parser 和业务反查入口必须删除。

### 1.1 集合读取覆盖

初始静态基线中直接链式调用审计集合读取接口的访问点共 34 处，已全部归入本合同；接口定义、测试及同一接口的间接业务调用不重复计入。这是改造前的覆盖统计，不是当前生产读取数量。

| 读取方式 | 访问点数 | 下游覆盖 |
| --- | ---: | --- |
| `audit_logs().find_by_id` | 25 | B01—B25 |
| `audit_logs().find_command_receipts_by_ids` | 4 | 通用回执 Service、工作流 adapter、回款提交、两类冲正提交；业务见 A01—A18 |
| `audit_logs().list_successful_by_resource` | 2 | 工作流 adapter 和库存调整撤回；业务见 C07、C08 |
| `audit_logs().list_work_item_creation_audits` | 1 | 创建人读取共享入口；业务见 C01—C06 |
| `audit_logs().search_logs` | 1 | 审计列表，见 E01 |
| `audit_events().search_audit_events` | 1 | 身份治理审计列表，见 E02 |

已补充检查原始集合名、集合常量、`AuditLog`/`AuditEvent` 类型、仓储别名、Port 实现、消息解析器及 `audit_id`/`audit_event_id` 用途。原始 `audit_events` 集合写入存在于角色授权仓储；未发现额外的生产反向业务读取。

资金列表另有两处原始聚合反查：回款登记人和付款提交执行人筛选。前者现读取 `CustomerReceipt.created_by`；后者现读取独立的 `SupplierPayment.posted_by` / `posted_at`。回款结算申请提交人仍读取 `ApprovalSubjectSnapshot.payload.submitted_by`。这两处补充检查不改动原 A—E 编号，验收见[实施登记 §5.1](audit-business-dependency-implementation.md#51-资金经办查询补充约束)。

### 1.2 初始基线关键事实

1. 初始 `audit_logs` 同时承载人类操作日志和机器命令协议，其 [ID 唯一索引](../backend/crates/erp-audit/src/indexes.rs) `uk_audit_logs_id` 参与确定性回执防重。现由独立领域回执唯一约束承担命令防重，日志约束只保护事件身份。
2. 初始通用命令指纹保存在 `message` 的 `command_fingerprint=` 段；其他业务还使用 `command_sha256=`、分隔符结果及 JSON 信封，详见 §2.6。现结构化回执保存规范化身份和结果，不解析展示消息。
3. 初始六类资金单据的创建人经审计进入 `ObjectFact.created_by`，随后参与工作项访问判断。五类实体原有 `created_by`，供应商付款原没有该字段；现六类均直接读取领域创建人，详见 C01—C06。
4. `SeparationAuditFact` 及批量职责分离查询没有发现生产调用。不得将其列为已经运行的审计职责分离链路。
5. 审批命令和供应商接口连接的部分命令已经使用独立回执；不得将这些实现整体改回审计回执，见 §2.5。

## 2 初始静态依赖清单与替代要求

本节保留改造前源码的动作、调用位置、消息及原结果语义，用于逐项核对覆盖。A、B、C、D 的原读取和解析器已经直接切换或删除，不作为现行运行合同；本节的替代要求与 §1.0、§3—§6 共同约束当前交付。

### 2.1 A 类 通用命令回执依赖

初始共同链路为 `CommandReceiptServiceExt::committed_resource_id` → `find_command_receipts_by_ids` → `AuditLog::pick_committed_resource_id` → `CommandReceipt::match_fact`。旧链读取审计 ID、操作人、动作、资源类型、成功标志、消息指纹及结果资源 ID；A04、A12、A13 曾直接在调用方 Executor 中批量查询，A17、A18 曾经工作流 Port 接入相同集合。以上审计回执接口及 parser 已退出；当前回放读取拥有领域的独立结构化回执。

迁移要求：每项必须改为拥有领域的结构化命令回执；同一动作的首次查询、事务内查询、异常后的结果恢复均须同步迁移。返回“原结果快照”或“结果对象的当前视图”沿用各入口现有语义，不得统一改成一种。

| 编号 | 业务命令 | 已核实入口与读取位置 | 目标拥有领域及必须保留的结果 |
| --- | --- | --- | --- |
| A01 | 登记进项发票 `purchase_invoice_allocation.post` | [payable/invoice.rs](../backend/crates/erp-processes/src/finance_posting/payable/invoice.rs)，`register_purchase_invoice`，58、143 行 | `erp-finance`；发票 ID、原登记结果与分配事实 |
| A02 | 提交供应商付款 `supplier_payment.commit` | [payable/payment.rs](../backend/crates/erp-processes/src/finance_posting/payable/payment.rs)，`commit_supplier_payment_with_assets` / `committed_payment_view` | `erp-finance`；付款 ID、附件是否已提交的返回语义，禁止重复付款或分配 |
| A03 | 新建并提交客户回款 `customer_receipt.commit` | [customer_receipt.rs](../backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt.rs)，`replay_committed_receipt` / `recover_committed_receipt` | `erp-finance`；回款 ID及已提交结果 |
| A04 | 提交已有回款 `customer_receipt.submit` | [customer_receipt_submit.rs](../backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt_submit.rs)，`committed_submit` / `matches_committed_submit` | `erp-finance`；原单与原请求指纹；保留授权先行及同 Executor 查询 |
| A05 | 登记并过账销项发票 `invoice.commit` | [receivable/invoice.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice.rs)，`commit_invoice_with_assets` / `committed_invoice_view`；[invoice_attachments.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_attachments.rs) 构造身份 | `erp-finance`；发票 ID与当前附件归一化；旧附件指纹 parser 删除 |
| A06 | 提交开票申请 `sales_invoice_request.submit` | [invoice_request/submit.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_request/submit.rs)，`submit_invoice_request` | `erp-finance`；申请 ID及绑定、启动结果 |
| A07 | 撤回开票申请 `sales_invoice_request.cancel` | [invoice_request/cancel.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_request/cancel.rs)，`cancel_invoice_request` | `erp-finance`；原申请撤回结果；初始旧身份曾将申请 ID 传入 `resource_type` 槽，现命令明确区分资源类型和申请 ID，不保留旧异常槽解码 |
| A08 | 新建并提交客户退款 `customer_refund.commit` | [customer_refund.rs](../backend/crates/erp-processes/src/reverse_flow/customer_refund.rs) → [customer_refund/commit_source.rs](../backend/crates/erp-processes/src/reverse_flow/customer_refund/commit_source.rs)，`customer_refund_commit_replay` | `erp-returns`；退款 ID，回放仍验证退款读取与提交资格 |
| A09 | 新建并提交供应商退款 `supplier_refund.commit` | [supplier_refund.rs](../backend/crates/erp-processes/src/reverse_flow/supplier_refund.rs) → [supplier_refund/commit_source.rs](../backend/crates/erp-processes/src/reverse_flow/supplier_refund/commit_source.rs)，`supplier_refund_commit_replay` | `erp-returns`；退款 ID及原来源事实 |
| A10 | 新建并提交回款冲正 `receipt_reversal.commit` | [receipt_reversal/commit.rs](../backend/crates/erp-processes/src/reverse_flow/receipt_reversal/commit.rs)，`commit_receipt_reversal` | `erp-returns`；冲正单 ID；禁止重复形成冲正 |
| A11 | 新建并提交付款冲正 `payment_reversal.commit` | [payment_reversal.rs](../backend/crates/erp-processes/src/reverse_flow/payment_reversal.rs)，`commit_payment_reversal` | `erp-returns`；冲正单 ID及原付款关系 |
| A12 | 提交已有回款冲正 `receipt_reversal.submit` | [reversal_submit.rs](../backend/crates/erp-processes/src/reverse_flow/start_approval/reversal_submit.rs)，`committed_reversal_submit`；请求合同见 [submit_reversal.rs](../backend/crates/erp-returns/src/dto/submit_reversal.rs) | `erp-returns`；绑定原请求版本的精确命令身份，不能只用 BPM 启动回执替代 |
| A13 | 提交已有付款冲正 `payment_reversal.submit` | 同 A12 的共享读取实现，独立的付款冲正请求身份 | `erp-returns`；保留原账号、静态权限、资金来源及经办职责校验顺序；未知结果只查证 |
| A14 | 提交客户验收 `customer_acceptance.commit` | [customer_acceptance/commit.rs](../backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/commit.rs)，`commit_customer_acceptance` | `erp-fulfillment`；验收 ID、销售单关系及完成任务事实 |
| A15 | 冲正客户验收 `customer_acceptance.reverse` | [customer_acceptance/reverse.rs](../backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/reverse.rs)，`reverse_customer_acceptance` | `erp-fulfillment`；反向验收 ID、原验收及原因身份 |
| A16 | 完成停止供应异常任务 `supplier_offering.supply_exception.complete` | [offering/exception.rs](../backend/crates/erp-processes/src/supply_governance/offering/exception.rs)，`complete_supply_exception_task` / `replay_supply_exception_completion` | `erp-supply`；任务 ID、正式处理决定及来源，不得在回放时恢复供给状态 |
| A17 | 通用工作项转交 `work_item.reassign` | [reassign.rs](../backend/crates/erp-workflow/src/service/work_item/reassign.rs) → [write.rs](../backend/crates/erp-workflow/src/service/work_item/write.rs)，`idempotent_replay` → [WorkflowAudit adapter](../backend/crates/erp-processes/src/adapters/workflow/audit.rs) | `erp-workflow`；工作项 ID、目标人、原版本、原因指纹；保留管理范围与任务类型守卫 |
| A18 | W29 工作项受控关闭 `work_item.close` | [close.rs](../backend/crates/erp-workflow/src/service/work_item/close.rs) → 同 A17 的 `idempotent_replay` | `erp-workflow`；工作项 ID、关闭原因、替代任务及领域关闭证据；覆盖 D03 |

### 2.2 B 类 直接读取审计的专用回执

下表一行对应初始基线的一处 `audit_logs().find_by_id` 读取实现，共 25 处。各项现已改读命令拥有领域的回执集合；表中方法名、行号和机器协议用于核对旧行为，正式业务事实交叉验证继续保留。

| 编号 | 业务范围 | 读取证据 | 审计承载内容及替代要求 |
| --- | --- | --- | --- |
| B01 | 历史导入业务确认完成 | [import_apply/complete.rs](../backend/crates/erp-processes/src/import_apply/complete.rs)，`replay_confirmation_completion`，183 行 | 指纹及确认结果、任务版本；迁至 `erp-import` 回执，继续校验确认记录、决定及任务完成状态 |
| B02 | 历史导入开始应用、取消待处理、重试失败项 | [import_apply/execution.rs](../backend/crates/erp-processes/src/import_apply/execution.rs)，`replay_import_execution`，101 行 | `StartApply` / `CancelPending` / `RetryFailed` 的批次与后台任务结果；保存动作、批次版本、任务标识及结果状态，允许后台进度按既有规则继续推进 |
| B03 | W29 非终结动作、任务完成、直接对账差异决定 | [task_decision/guard.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/guard.rs)，`replay_receipt`，23 行；调用者为 [action.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/action.rs)、[complete.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/complete.rs)、[direct.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/direct.rs) | JSON `ReceiptEnvelope<T>` 保存指纹和强类型结果；迁至 `erp-integration`。覆盖查原结果、重放原动作、重新归集、关联补偿、追加证据，以及直接决定的非终结/终结分支 |
| B04 | 供应商供给责任交接 | [offering_handover.rs](../backend/crates/erp-processes/src/offering_handover.rs)，`replay`，150 行 | `message` 指纹后重读当前维护人、组织和版本；迁至 `erp-supply`，保持当前视图返回与当前权限重验 |
| B05 | 销售建单，包含建单并提交分支 | [order_to_cash/command/create.rs](../backend/crates/erp-processes/src/order_to_cash/command/create.rs)，`replay_sales_order_creation`，467 行 | 创建动作、操作人、指纹、销售单 ID；迁至 `erp-sales`，保留 `stable.created_by` 一致性与当前访问重验 |
| B06 | 销售提交 | [order_to_cash/command/submit.rs](../backend/crates/erp-processes/src/order_to_cash/command/submit.rs)，`replay_sales_submission`，383 行 | 指纹及提交快照 ID；迁至 `erp-sales`，返回原提交快照与行，校验销售单及提交人 |
| B07 | 销售责任交接 | [order_to_cash/handover.rs](../backend/crates/erp-processes/src/order_to_cash/handover.rs)，`replay_sales_handover`，189 行 | 指纹、销售单 ID；迁至 `erp-sales`，保持当前责任人与开放任务视图语义 |
| B08 | 按采购依据创建采购草稿 | [creation_basis/create.rs](../backend/crates/erp-processes/src/procure_to_pay/creation_basis/create.rs)，`replay_creation`，654 行 | `PurchaseCommandReceipt<CreationReceipt>`；迁至 `erp-procurement`，保留创建结果和单据存在性验证 |
| B09 | 保存采购草稿 | [procure_to_pay/draft_edit.rs](../backend/crates/erp-processes/src/procure_to_pay/draft_edit.rs)，`replay_saved_draft`，349 行 | `SaveDraftReceipt` 及锁版本；迁至 `erp-procurement`，允许当前版本高于回执版本但不能低于它 |
| B10 | 采购选源批量建单及库存供给分配 | [procure_to_pay/sourcing_create.rs](../backend/crates/erp-processes/src/procure_to_pay/sourcing_create.rs)，`replay_sourcing`，427 行 | `SourcingReceipt` 保存采购单集合、库存分配和任务结果；迁至 `erp-procurement`，覆盖子建单回执及 D02 的预占来源关系 |
| B11 | 采购提交 | [procure_to_pay/submission.rs](../backend/crates/erp-processes/src/procure_to_pay/submission.rs)，`replay_purchase_submit`，282 行 | 初始按当前/历史候选 ID 读取 `PurchaseSubmitReceipt`；现直接读取 `purchase_command_receipts` 的当前稳定命令，保留审批、任务与版本结果，旧候选删除 |
| B12 | 作废采购草稿 | [procure_to_pay/void_order.rs](../backend/crates/erp-processes/src/procure_to_pay/void_order.rs)，`replay_void_draft`，329 行 | `VoidDraftReceipt`；迁至 `erp-procurement`，继续核对当前作废状态及版本 |
| B13 | 商品维护责任交接 | [product_handover.rs](../backend/crates/erp-processes/src/product_handover.rs)，`replay`，150 行 | 指纹及当前商品责任视图；迁至 `erp-catalog`，保留目标资格、组织及当前访问控制 |
| B14 | 供应商责任交接 | [supplier_profile/handover.rs](../backend/crates/erp-processes/src/supplier_profile/handover.rs)，`replay_supplier_handover`，169 行 | 指纹后重读供应商当前责任；迁至 `erp-supplier` |
| B15 | 供应商能力责任交接 | 同文件 `replay_capability_handover`，196 行 | 指纹后重读能力负责人及版本；迁至 `erp-supplier`，不能与供应商账户交接共用不带目标类型的身份 |
| B16 | W26 供应商订单任务完成 | [supply_execution/complete.rs](../backend/crates/erp-processes/src/supply_execution/complete.rs)，`replay_task_completion`，160 行 | 完成收据包含终结动作 ID、任务/订单结果及处理结论；迁至 `erp-supply`，继续核对 `supplier_order_actions`，覆盖 D01 |
| B17 | 供应商履约订单跟进责任交接 | [supply_execution/handover.rs](../backend/crates/erp-processes/src/supply_execution/handover.rs)，`replay`，207 行 | 指纹及当前跟进人/版本；迁至 `erp-supply`，保留当前规范化指纹及权限重验，旧说明后缀 parser 删除 |
| B18 | W26 直接调查和任务内调查 | [supply_execution/investigate.rs](../backend/crates/erp-processes/src/supply_execution/investigate.rs)，`replay_investigation`，525 行 | 调查收据、证据 ID、操作号及任务版本；迁至 `erp-supply`，两个入口必须保持身份区分，覆盖 D01 |
| B19 | 供应商 API 能力配置变更 | [supplier_api/command.rs](../backend/crates/erp-processes/src/supply_governance/supplier_api/command.rs)，`update_capabilities`，226 行 | 确定性审计 ID 与指纹；迁至 `erp-supply` 独立回执，保留返回 `audit_event_id` 的关联合同；不能因其他连接命令已有回执而漏掉该入口 |
| B20 | 供应商结算差异正式决定 | [supply_settlement/difference.rs](../backend/crates/erp-processes/src/supply_settlement/difference.rs)，`replay_difference_decision`，121 行 | 操作号、结算单/差异版本及结果；迁至 `erp-supply`，保留差异、明细与单头联合核验 |
| B21 | 供应商结算草稿刷新 | [supply_settlement/draft.rs](../backend/crates/erp-processes/src/supply_settlement/draft.rs)，`replay_refresh`，254 行 | 请求号、来源快照、版本及明细/差异数量；迁至 `erp-supply`。来源未变化时仍写回执，不能用“无字段差异”省略此分支 |
| B22 | 供应商结算责任交接 | [supply_settlement/handover.rs](../backend/crates/erp-processes/src/supply_settlement/handover.rs)，`replay_handover`，180 行 | 指纹及当前制单人/组织/版本；迁至 `erp-supply` |
| B23 | 供应商结算差异处理人改派 | 同文件 `replay_handler`，199 行 | 指纹及当前差异处理人/版本；迁至 `erp-supply`，与结算责任交接保留不同动作身份 |
| B24 | 供应商结算提交财务复核 | [supply_settlement/review.rs](../backend/crates/erp-processes/src/supply_settlement/review.rs)，`replay_review_submission`，276 行 | 操作号、冻结主题/单头版本及工作项 ID；迁至 `erp-supply`，继续交叉验证任务和结算单 |
| B25 | 供应商结算复核决定 | 同文件 `replay_review_decision`，311 行；写入见 [review_posting.rs](../backend/crates/erp-processes/src/supply_settlement/review_posting.rs) | 复核结果、任务版本、结算版本及成本差额；迁至 `erp-supply`，保留原复核人校验、制单人分离规则及成本写序；结果不能由展示文案恢复 |

### 2.3 C 类 业务身份与取消终态依赖

C01—C06 的初始公共链路为：

`authority/funds/{payments,receipts}.rs` 或 `funds_document_brief/{payment,receipt}.rs` → `load_created_by_from_audit` → `list_work_item_creation_audits` → [funds/mapping.rs](../backend/crates/erp-read-models/src/workbench/authority/funds/mapping.rs) → `ObjectFact.created_by` → [has_object_participation](../backend/crates/erp-workflow/src/service/work_item/access.rs)。旧审计创建人入口现已删除，权威事实和简报直接读取领域创建人。最后一步仍判断创建人是否为当前账号，或账号是否参与根单据；空创建人不能授予参与资格。该条件不能替代 RBAC、DataScope 或任务责任条件。

| 编号 | 业务对象 | 初始读取及来源 | 当前替代合同 |
| --- | --- | --- | --- |
| C01 | 客户回款 | 创建审计首个操作人进入资金权威事实和工作台简报 | 直接读取 [CustomerReceipt.created_by](../backend/crates/erp-finance/src/entity/receivable/customer_receipt.rs)，不保留审计核对或回退 |
| C02 | 供应商付款 | 同上 | 初始 [SupplierPayment](../backend/crates/erp-finance/src/entity/payable/supplier_payment.rs) 没有创建人字段；现已新增不可变 `created_by`，新付款从已认证命令人写入，不实施历史回填 |
| C03 | 客户退款 | 同上 | 读取 [CustomerRefund.created_by](../backend/crates/erp-returns/src/entity/returns/customer_refund.rs)；不得改用可变化的当前经办人 |
| C04 | 供应商退款 | 同上 | 读取 [SupplierRefund.created_by](../backend/crates/erp-returns/src/entity/returns/supplier_refund.rs) |
| C05 | 回款冲正 | 同上 | 读取 [ReceiptReversal.created_by](../backend/crates/erp-returns/src/entity/returns/receipt_reversal.rs) |
| C06 | 付款冲正 | 同上 | 读取 [PaymentReversal.created_by](../backend/crates/erp-returns/src/entity/returns/payment_reversal.rs) |
| C07 | 库存调整撤回的原命令操作人 | [cancel_runtime.rs](../backend/crates/erp-processes/src/inventory_adjustment/cancel_runtime.rs)，`committed_cancel_actor`，238 行；按动作和消息中的实例前缀筛选，要求唯一原操作人 | 在 `erp-inventory` 保存撤回命令事实，包含调整单、审批实例、原操作人、命令身份、原因及结果引用；保留与现有审批回执的关联和回放身份校验 |
| C08 | 受阻审批取消结果恢复 | [cancel_blocked.rs](../backend/crates/erp-workflow/src/service/approval/execution/runtime_service/cancel_blocked.rs)，`load_cancel_blocked_terminal_facts`，236 行；从 `approval.cancel_blocked` 消息解析 `execution=… reason=…`，要求唯一审计并交叉验证执行、任务及实例终态 | 在 `erp-workflow` 保存结构化取消事实或扩展其拥有的 ERP 取消结果记录；保留 actor、execution、reason、blocker 与终态时间关系。已有 `ApprovalCommandReceipt.result_ref` 本身不足以替代全部事实 |

C01—C06 的两套消费入口必须同时迁移：[权威付款读取](../backend/crates/erp-read-models/src/workbench/authority/funds/payments.rs)、[权威回款读取](../backend/crates/erp-read-models/src/workbench/authority/funds/receipts.rs)、[付款简报](../backend/crates/erp-read-models/src/workbench/funds_document_brief/payment.rs)、[回款简报](../backend/crates/erp-read-models/src/workbench/funds_document_brief/receipt.rs)。不得只修展示而保留命令权威读取的审计依赖，也不得只修权威读取而使两套投影身份不同。

### 2.4 D 类 审计身份与业务关联耦合

下列路径不一定查询审计正文，但重建审计 ID 时会改变业务标识或来源关系，必须纳入迁移。

| 编号 | 场景 | 初始基线行为 | 当前替代要求 |
| --- | --- | --- | --- |
| D01 | W26 调查及任务完成 | [supply_execution/investigate.rs](../backend/crates/erp-processes/src/supply_execution/investigate.rs)、[complete.rs](../backend/crates/erp-processes/src/supply_execution/complete.rs) 使用 `stable_evidence_id` / `stable_internal_idempotency_key`；算法在 [supplier_fulfillment/receipt.rs](../backend/crates/erp-supply/src/service/supplier_fulfillment/receipt.rs)，输入是确定性审计 ID | 由稳定命令身份派生新证据与内部幂等键；正式动作及原结果仍交叉核验，不实施旧审计 ID 映射或旧算法兼容 |
| D02 | 采购选源中的现有库存预占 | [sourcing_create/stock_posting.rs](../backend/crates/erp-processes/src/procure_to_pay/sourcing_create/stock_posting.rs)，`build_pending_line` 将 `audit_id` 写入 `StockReservationEntry.source_document_id` | `source_document_id` 直接保存选源命令 ID；预占 `source_type=ExistingStock` 及 `source_allocation_id` 保存正式分配身份。不保留审计 ID 来源，不实施历史映射 |
| D03 | W29 受控关闭产生对账结论 | [work_item/close.rs](../backend/crates/erp-workflow/src/service/work_item/close.rs) 将当前回执 ID 传给领域关闭；[adapters/workflow/w29_close.rs](../backend/crates/erp-processes/src/adapters/workflow/w29_close.rs) 用该 ID 的指纹生成 `w29-close-…` 结论 ID | A18 拆分后仍沿用稳定命令身份；不可将随机新审计 ID 用于结论去重。原结论序号、关闭类型及证据引用保持不变 |

### 2.5 E 类 正常审计用途与已有独立机制

| 编号 | 场景 | 核实结论及处理要求 |
| --- | --- | --- |
| E01 | 系统操作日志列表 | [admin/audit_log.rs](../backend/apps/web-api/src/core/handler/admin/audit_log.rs) → `AuditLogService::audit_log_list` → `search_logs`；属于合法审计查询。增加中文及结构化响应，前后端同步更新，不建立旧版响应兼容窗口 |
| E02 | 身份治理审计列表 | [access_control/mod.rs](../backend/crates/erp-identity/src/service/access_control/mod.rs) 的 `audit_event_list` → `audit_events.search_audit_events`；前端 [access-audit](../erp-client/features/access-audit) 读取该语义。动作注册表归身份域，不得把业务日志无条件加入其选项 |
| E03 | 供应商连接通用治理命令 | [supplier_api/command.rs](../backend/crates/erp-processes/src/supply_governance/supplier_api/command.rs) 的 `replay_command` 读取 `supplier_api.command_receipt`；[SupplierConnectionCommandReceipt](../backend/crates/erp-supply/src/entity/supplier_api/governance/receipt.rs) 已存独立指纹和结果。`audit_event_id` 是关联引用，保留即可；B19 使用单独的 `supply_command_receipts` 能力修改结果，不与本项混淆 |
| E04 | 审批命令 | [ApprovalCommandReceipt](../backend/crates/bpm/src/model/command_receipt.rs) 已独立建模，由 `erp-workflow` 的 `approval_command_receipts` 集合和 [唯一索引](../backend/crates/erp-workflow/src/indexes/bpm.rs) 持久化。继续复用；C08 的完整取消终态保存在 `approval_cancellation_facts`，须独立验证，不能只凭原审批回执核销 |
| E05 | 供给创建、修订、可供性修改 | [offering/commit.rs](../backend/crates/erp-processes/src/supply_governance/offering/commit.rs) 先持久化领域命令再写同事务审计；领域实现位于 [supplier_offering](../backend/crates/erp-supply/src/service/supplier_offering)。不属于查日志恢复命令；A16、B04 单列 |
| E06 | 供应商结算草稿创建 | [supply_settlement/draft.rs](../backend/crates/erp-processes/src/supply_settlement/draft.rs) 先走 `prepare_scoped_statement` 的领域重放分支；初始审计曾包含机器格式结果，未发现读取该创建审计的生产分支。不得与 B21 的刷新混淆；统一持久化边界生成安全结构化中文事件，新的展示记录不写机器协议 |
| E07 | API 返回的审计编号 | 供应商 API 结果的 `audit_event_id`、导入结果中的审计编号是对外关联合同。前端 [supplier-api commands](../erp-client/features/supplier-api-connections/api/commands.ts) 映射为 `auditEventId`。回执解耦不授权删除或重命名这些响应字段；它们不等同于额外的审计正文反查 |
| E08 | 登录、敏感读取与普通业务成功审计 | 登录在 [auth/login.rs](../backend/apps/web-api/src/core/handler/auth/login.rs) 采用 best-effort；敏感读取及正式写入沿各自合同保留。不能统一改成所有失败可忽略，也不能把登录强行加入业务写事务 |

### 2.6 已退出的基线消息协议

| 基线协议 | 原消费方 | 当前替代与删除要求 |
| --- | --- | --- |
| `command_fingerprint=…` 及其历史兼容格式 | A 类，[application-core/command.rs](../backend/crates/application-core/src/command.rs) | 保存当前命令身份、版本化规范化指纹、资源及 actor 校验和损坏分类；删除旧候选 ID 与审计消息 parser |
| `command_sha256=…`，可能允许说明后缀 | 销售建单/提交/交接、商品/供给/供应商/能力/履约责任交接、供应商能力修改 | 保留各动作当前载荷规范化规则，保存安全摘要与结构化身份；删除说明后缀 parser 及旧原始键候选定位，不用宽松摘要包含判断代替匹配 |
| 采购版本化回执信封 | B08—B12，[purchase_order/command_receipt.rs](../backend/crates/erp-procurement/src/entity/purchase_order/command_receipt.rs) | 独立保存完整强类型 payload、schema 版本及错误分类；只定位当前命令，不保存旧审计候选 |
| `command_sha256=…;result=…` | 历史导入业务确认、供应商结算 | 由独立回执恢复强类型字段与枚举；保留任务版本、批次/单据版本和结果数量，旧 result 消息 parser 删除 |
| `command_sha256=…;execution=…` | 历史导入执行，[import_apply/execution.rs](../backend/crates/erp-processes/src/import_apply/execution.rs) | 由独立执行结果保存动作、批次/试算/后台任务版本及状态、影响项数和下一步；旧 execution 消息 parser 删除 |
| `fp=…;e=…;o=…;t=…` / `fp=…;a=…;o=…;t=…;r=…` | W26 调查/任务完成，[supply_execution/receipt.rs](../backend/crates/erp-processes/src/supply_execution/receipt.rs) | 由强类型回执保存证据或终结动作、订单/任务版本与结论；结构化 schema 损坏明确拒绝，分隔符 parser 删除 |
| JSON `ReceiptEnvelope<T>` | B03，[task_decision/guard.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/guard.rs) | 独立保存完整强类型结果及身份检查；旧审计 JSON 信封 parser 删除，不从展示正文恢复业务 |
| `execution=… reason=…`；库存撤回实例前缀 | C08、C07 | 独立取消事实交叉校验实例、执行、原操作人、原因、历史任务、唯一性和终态；消息前缀及取消文本 parser 删除，不实施一次性迁移解码 |

新回执写入不得使用审计 `message` 承载机器协议。本次未上线实施删除旧协议解析代码，不交付兼容模块或迁移工具；本表只保留旧协议静态盘点事实。

### 2.7 未发现生产调用的接口

以下接口在初始基线位于 `erp-audit/repository/audit_log.rs`，当时未发现生产调用；本次现已删除定义、实现及相关导出，不得为业务资格或回放重新接入。

| 接口 | 处理要求 |
| --- | --- |
| `list_separation_facts_by_resources` / `SeparationAuditFact` | 已删除；不登记为现行职责分离依赖，不得重新接入业务规则 |
| `list_successful_work_item_fact_audits` | 已删除；原 `list_work_item_creation_audits` 也随 C 类直接读取切换而删除 |
| `list_master_mapping_task_history` | 已删除；不登记为当前映射业务依赖 |
| `list_master_mapping_task_histories` | 已删除；不得仅凭原“时间线”注释声称页面已接入 |

`SpecificationSignatureAuditRow` 位于商品规格签名检查仓储，读取 `SKUS`，属于数据检查命名，不是审计日志依赖。

## 3 目标职责与数据合同

### 3.1 归属

| 层次 | 必须承担的职责 | 禁止承担的职责 |
| --- | --- | --- |
| `application-core` | 通用命令身份、指纹版本、结果分类、执行上下文及窄的共享合同 | 业务实体、业务集合访问、中文业务规则和跨域查询 |
| 拥有命令的领域 | 回执实体、结果 schema、仓储与索引；创建人、经办人、撤回/决定等业务事实；动作与可审计字段语义 | 依赖其他业务领域或 `erp-processes` |
| `erp-audit` | 审计记录实体、校验、存储、查询；接受已经明确的安全事件事实 | 向业务提供幂等结果、创建人或授权判断依据 |
| `erp-processes` | 跨域命令事务、事实到审计的 adapter、审计统一执行包装 | 将业务不变式移到通用日志框架，或直接访问外域集合 |
| `erp-read-models` | 读取领域事实形成工作台/列表投影；授权后的跨域显示组合 | 为创建人、经办人、状态及命令结果反查审计 |
| `persistence-core` | Executor、事务及通用持久化机制 | 审计动作、业务字段白名单和事件中文生成 |

各领域可以登记自己的 `<domain>_command_receipts`，共享身份与通用 repository 机制；已经存在的领域回执复用其集合。不得仅为复用而新增业务领域相互依赖，也不得把全部业务回执实体集中塞入 `erp-support` 或公共基础 crate。

### 3.2 独立命令回执

每个回执 schema 至少明确以下信息：

| 字段组 | 合同 |
| --- | --- |
| 命令身份 | 稳定 `command_id`、动作、作用域/目标、操作人及幂等键摘要；明确唯一约束组成 |
| 载荷身份 | 规范化请求指纹及算法/schema 版本；保留既有金额、数量、附件归一化规则 |
| 结果 | 结果资源引用、必要的强类型结果快照、相关业务/任务版本；各业务保留原结果恢复语义 |
| 关联 | 审计事件 ID、原审批回执/后台任务/证据引用；这些引用不得反过来成为读取审计正文的理由 |
| 历史兼容 | 本次未上线范围不适用；不保存旧回执别名或审计协议来源，不反推历史幂等键 |

同步数据库命令的成功回执与业务事实同事务提交。外部任务已有的 `Processing`、`Rejected`、`Unknown` 生命周期由拥有领域继续解释，不得套用“HTTP 成功即命令最终成功”。回执保存期限必须覆盖约定的重试与追溯期限，不得沿日志归档策略删除仍承担去重作用的回执。

### 3.3 身份与终态事实

1. `created_by` 记录不可变创建人；责任交接不得覆盖它。`handled_by`、审批决定人、当前责任人、付款执行人等字段各自保留语义，禁止互相替代。
2. C01、C03—C06 直接使用已有领域字段；空值或非法身份明确失败，不授予参与资格，不从审计、当前经办人或当前账号补齐。
3. C02 由创建命令新增不可变认证创建人字段，直接供两套投影读取；本次不做旧数据回填，不保留身份审计 fallback。
4. C07、C08 保存不可变、结构化的取消操作事实。原回执、目标、操作者、原因、执行节点及终态必须相互验证；不能将终态校验缩减为“有一条回执”。
5. 静态权限、DataScope、对象参与关系、当前任务责任、审批规则分别执行。回执命中不能成为绕过这些校验的通行证；迁移不得改变既有错误顺序和不可见对象的响应口径。

## 4 自动审计执行合同

### 4.1 结构化事件

审计事件必须保存稳定动作代码、动作/schema 版本、操作人身份与安全名称快照、目标类型/ID/业务编号快照、业务结果、允许记录的字段变化、关联命令/请求以及发生时间。审计响应与前端展示同步更新，不建立历史字段兼容窗口。

后台认证复用 `validate_session` 已取得的 `AccountCore.name`，在 `AuditActor` 中冻结可选名称快照，再随执行上下文、各领域 Prepared 结果和审计 adapter 传入成功事件及尝试记录。名称去除首尾空白、长度不超过 128 个字符且不得含控制字符；未取得名称时保持 `None`，已提供但不合法的名称明确拒绝；不额外查询当前名称补成历史名称。名称仅用于安全展示，不替代 actor ID、账号类型或授权身份。

中文动作、字段名、状态值和数量/金额格式来自明确的领域注册项。不得从权限目录、Rust 函数名或 HTTP 路径推导业务含义。身份域 `audit_events` 继续遵守 [查询与审计目录合同 §11.4](query-selector-decoupling-contract.md)，业务事件目录单独登记或以明确命名空间聚合。

服务履约确认的业务记录至少回答“谁确认了哪张履约单、确认状态如何变化、服务结果与数量是什么”。服务结果和命令执行结果必须分开：确认了一条服务失败事实，也可能是成功执行的确认命令。

字段变化只对显式允许记录的业务投影计算；敏感值记录“已变更”或合同允许的脱敏值。禁止将完整实体、请求体、密文、凭据及银行卡信息直接序列化到审计。显示名改动不得改变历史快照；缺少历史字段时显示缺失，不得用当前名称伪造历史。

### 4.2 写入机制

1. 在本域 Service 或命名 Process 的用例边界接入统一执行包装；沿用现有 [run_audited](../backend/crates/erp-processes/src/audit/transaction.rs) 的同事务原则，扩展为接收类型化动作及执行后事实，不再要求调用方构造完整 `AuditLog`。
2. 动作元数据和静态输入在写入前校验；必须依赖执行结果的事实在事务内、提交前完成校验。审计写入失败必须沿原合同使正式业务写入失败。
3. 已有调用方事务时复用其 `Executor`，禁止包装器嵌套开启事务。单域经自身窄 Port 接入，跨域 adapter 在组合层装配。
4. 新鲜执行成功后写一次业务事件；幂等重放返回原结果，不追加第二条“业务再次成功”的事件。无字段变化但产生有效命令结果的 B21 等场景按动作合同记录，不得用通用 diff 判定是否应写回执。
5. 一个命令修改多个对象时，以稳定 `command_id` 关联主事件和必要子事件，保留顺序及事件序号；不得依赖“每次 repository.update 自动记一次”解释完整业务动作。
6. 业务代码负责明确动作和必要语义事实；框架负责上下文补齐、构造、白名单差异、持久化及展示合同。先用普通 Rust 类型/trait 实现，再在稳定规则上用 derive/属性宏生成重复代码。
7. [依赖登记](../backend/scripts/audit-boundaries.json) 约束允许的审计集合读写；[入口登记](../backend/scripts/audit-entrypoints.json) 按精确路径/函数登记 `Audited` 或带原因的 `Exempt`、执行阶段及实际调用符号、次数和位置数。[门禁](../backend/scripts/audit_boundaries.py) 拒绝未登记入口、过时登记或超出登记的调用；豁免仅适用明确授权展示和限定初始化入口，禁止普通业务默认豁免。该登记覆盖显式审计调用边界，不能证明全仓所有业务写入或运行审计行为；V14 仍须检查真实调用链和实际生产编排库单测，不能凭登记条数核销。
8. HTTP 中间件可收集请求关联、耗时和拒绝信息；不得独自承担成功业务审计，因为后台任务、CLI、重放和跨域事务具有独立边界。
9. 权限拒绝与执行失败记录使用独立尝试事件语义。[`audit_attempts`](../backend/crates/erp-audit/src/repository/attempt.rs) 由审计领域拥有，在原事务返回后通过独立 `NoTransaction` 持久化，不复用已结束的事务，也不替代成功事件或领域命令回执。尝试只保存安全类型化上下文和 `Rejected` / `Failed` / `Unknown` 分类，不复制敏感错误正文；尝试写入失败保持原命令错误，不再次执行业务。`OutcomeUnknown` 记录未知并保留原命令查证入口，不得误标为失败或自动重放。
10. 保留身份策略事务的提交后刷新、未知结果失败关闭及请求取消收尾合同。普通 `run_audited` 与身份策略事务必须分别评估生命周期，禁止用一个通用包装静默替换全部特殊行为。

### 4.3 展示与查询

1. 对业务人员展示中文动作、操作人、业务编号、结果及字段变化；技术动作代码和追踪号置于详情。
2. 以结构化字段筛选和导出，不从自由文本解析业务状态。日志模板变化不得影响业务执行或回放。
3. 审计记录的可见性及关联对象链接执行相应查询授权；取得审计列表权限不自动取得银行信息、客户敏感资料或关联单据的读取资格。
4. 查询和模板渲染可以扩展，成功事件的最低事实仍须与业务事务一致；异步展示投影失败不能丢失原始审计事实。

## 5 实施顺序与核销合同

### 5.1 执行顺序

| 阶段 | 必须完成的工作 | 退出条件 |
| --- | --- | --- |
| M0 固定事实与合同 | 为 A01—A18、B01—B25、C01—C08、D01—D03 建立逐项状态，确认当前字段、结果语义、拥有领域及验收用例 | 每项具备拥有领域、新事实目标和验收义务；不得用全局“已完成”代替逐项状态 |
| M1 建立独立回执与事实 | 在拥有领域新增/复用结构化回执、创建人及取消事实；登记仓储、索引和稳定命令身份 | 新命令可独立匹配指纹和恢复强类型结果；业务事实、回执和成功审计同 Executor 写入，领域边界通过 |
| M2 直接切换业务读取 | A、B 切到领域回执；C 切到领域身份及取消事实；D 切到稳定命令来源 | 当前命令的结果、授权、参与关系、版本、任务及错误优先级保持；无审计 fallback 或旧回执双写 |
| M3 接入自动审计 | 用类型化动作及执行后事实接入统一边界，交付中文事件、查询及展示 | 普通业务函数不再手工拼装完整日志；后台、内部及 HTTP 入口登记审计或明确免记策略 |
| M4 删除旧依赖 | 删除生产旧消息 parser、业务反查入口及确认无调用的历史查询；增加依赖退出门禁 | 审计仅承担安全写入和授权展示；新回执及业务关联不依赖审计格式，适用库单测和质量门禁通过 |
| M5 核销 | 汇总每项代码、库单测、门禁及适用运行验收；同步实施登记和相关文档 | 所有适用验收有证据；未取得真实环境证据的项目保持未验证，不执行旧版回填/回退流程 |

本次无历史数据迁移、兼容 writer 或回退窗口。不得新增旧日志协议双写、legacy alias 回填或永久双读；新回执冲突和损坏明确失败，不得退回展示审计恢复业务。新增集合的索引与 Executor 约束仍按仓库规则执行。

### 5.2 分批实施顺序

| 批次 | 范围 | 必须交付的结果 |
| --- | --- | --- |
| P1 基础与样板 | A01、C01 及服务履约确认 | 独立财务回执样板、创建人领域读取样板、自动审计及中文事件样板；服务履约确认本身没有本清单内的日志回放依赖，不得凭空增加业务重试语义 |
| P2 资金与逆向 | A02—A13、C02—C06 | 完整资金回执及六类创建人投影解耦；覆盖当前附件归一化和精确原请求版本指纹 |
| P3 销售采购与主数据交接 | B04—B15、D02 | 销售/采购版本化结果、批量选源及各类交接；新库存分录来源直接使用可判别的稳定业务引用 |
| P4 供应链治理与结算 | A16、B16—B25、D01 | W26/W20/结算回执解耦；不重复改造 E03 已有独立回执 |
| P5 工作流履约与集成导入 | A14、A15、A17、A18、B01—B03、C07、C08、D03 | 任务/取消终态、验收、导入和 W29 全部解耦；保留后台与外部任务生命周期 |
| P6 收口 | E 类保留边界、§2.7 遗留接口、旧 parser 退出及所有事件目录 | 删除业务反查入口，统一安全中文展示，执行依赖门禁及文档同步 |

分批顺序用于安排工作，不允许只交付 P1 样板后宣称完成整份合同。本次各项直接执行 M0—M5，不等待历史兼容窗口；全部适用验收有证据后才能核销。

## 6 验收合同

### 6.1 必须覆盖的行为

| 编号 | 验收项 | 通过标准 |
| --- | --- | --- |
| V01 | 同键同载荷 | 返回原命令允许的结果形态；业务写入、资金流水、任务、附件及成功审计均不重复 |
| V02 | 同键异载荷 | 保留稳定冲突及原错误优先级；不覆盖旧回执，不退回审计掩盖冲突 |
| V03 | 旧版本重试 | 本次未上线实施不适用；不新增旧候选 ID、历史指纹或 A07 旧异常身份槽兼容 parser |
| V04 | 同事务与失败停止 | 正式业务事实、回执、审计使用同一个 Executor；任一步失败停止后续步骤；编排单测不得被描述为真实数据库回滚证明 |
| V05 | 并发提交与唯一性 | 并发 writer 对同一命令只形成一份有效业务结果；依赖新领域回执唯一约束，不以“先查后写”代替并发防护。真实并发未执行时保持未验证 |
| V06 | 未知提交结果 | 只查证原命令；恢复失败保持未知及原错误，不自动执行第二次写事务 |
| V07 | 身份与授权 | 六类创建人由领域事实形成；切换前后可见集合、参与资格及允许动作对等；RBAC、DataScope、任务责任分别保持 |
| V08 | 取消事实 | 缺失、重复或不匹配的 actor/实例/执行/原因/终态必须拒绝；C07、C08 不因存在回执就通过 |
| V09 | 专用结果 | 采购/结算/导入/W26/W29 的原版本、任务、证据、成本差额和结果状态无损迁移；覆盖 B21 无变化分支 |
| V10 | 标识与关联 | D01—D03 的新业务 ID、库存来源和结论证据由稳定命令或正式业务事实产生；API 审计编号保持可关联 |
| V11 | 展示与隐私 | 用中文说明人物、对象、动作和结果；业务字段名明确；秘密与完整敏感值不落日志；历史未知字段不伪造 |
| V12 | 回填与回退 | 本次未上线实施不适用；不新增历史回填、滚动兼容或旧二进制回退验收 |
| V13 | 依赖退出 | A、B、C 的生产业务读取不再访问审计；D 改为稳定命令来源；保留的审计查询仅为授权展示/追溯用途 |
| V14 | 自动记录覆盖 | 新增/迁移写用例必须登记事件或明确免记策略；HTTP、后台任务及内部用例均经过对应边界；多对象事件无重复遗漏 |

### 6.2 验证证据边界

1. 实现阶段遵守 [backend/AGENTS.md](../backend/AGENTS.md)：只新增/执行内联库单元测试，不新增、修改或执行集成测试、真实 Mongo/S3 验收例程。
2. 单元测试必须执行真实生产编排和解析逻辑，覆盖成功、失败、边界、回放零写入及同 Executor 传递；禁止复制实现后测试副本，禁止用源码字符串计数替代业务行为验证。
3. 文档本身只执行路径/内容核对与 `git diff --check`；不得因本合同已落盘声称 V01—V14 已通过。
4. V05、真实事务回滚及最终中文浏览器体验需要独立运行验收记录。用户明确要求 E2E 时，执行仓库隔离流程入口，开发源库仅只读复制；服务、种子、业务写入和清理绑定临时分片。只核销报告实际覆盖的入口与分支，运行结果登记到[实施登记 §10](audit-business-dependency-implementation.md#10-本次统一检查与未核销范围)；不得以全量通过替代未执行的并发、故障注入或事实错配矩阵。历史数据回填和滚动版本兼容按 §1.0 不适用。
5. 后续每批代码交付执行受影响 crate 的 fmt/check/库单测/clippy；公开签名、领域边界、BPM 和权限生成变动按仓库要求追加门禁。提交前执行仓库规定的全量质量门禁。

## 7 关联文档一致性要求

关联文档已按以下合同修订；后续修改必须同步保持这些边界，不得恢复初始基线中的旧表述。

| 文档及合同点 | 当前约束 | 执行要求 |
| --- | --- | --- |
| [backend/docs/audit-logging.md](../backend/docs/audit-logging.md) 的服务履约确认回执范围 | 服务确认本身没有跨请求回执；A、B 的其他命令具备独立命令身份与回执 | 保持局部用例范围，不得恢复“项目尚未提供跨请求幂等键”的全局表述 |
| 同文的请求取消与任务所有权 | [run_audited / run_audited_event](../backend/crates/erp-processes/src/audit/transaction.rs) 直接等待 `with_transaction`，不创建独立持有事务的 Tokio 任务 | 身份策略的特殊收尾分别验收；普通事务需要独立收尾时必须显式实现并验证，不得凭包装函数名称承诺该行为 |
| 同文的审计校验时点 | 事务前校验动作与身份；提交前校验执行后的安全结果事实 | 保持两阶段校验和审计失败阻止提交，不得要求执行前提供尚未发生的结果 |
| [query-selector-decoupling-contract.md §11.4](query-selector-decoupling-contract.md) 限定身份域审计事件目录 | 业务 `audit_logs` 是不同集合与语义 | 自动中文事件目录不得无条件复用身份审计候选；分别登记后由明确查询合同聚合 |

## 8 完成条件

1. A01—A18、B01—B25、C01—C08、D01—D03 均具有实施及验收证据；E 类合法用途与既有独立机制保留正确边界。
2. 审计展示文本、事件中文模板和审计归档策略的变化，不再影响命令去重、业务状态、原操作人识别或参与资格。
3. 普通业务函数不再重复构造 `AuditLog` 和调用审计仓储；必要的业务语义以类型化事实明确提供，由统一边界记录。
4. 本次未上线范围内的生产旧协议解析器、兼容双写和业务审计反查全部退出；不交付历史回填、旧版本兼容或回退机制，不通过清库完成代码解耦。
5. 未取得真实环境证据的验收项继续标为未验证，禁止以编译、单元测试、静态扫描或文档完成替代运行验收。
