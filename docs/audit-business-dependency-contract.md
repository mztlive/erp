# 审计业务依赖清单与解耦实施合同

本合同规定审计业务依赖的完整静态清单、替代事实来源、迁移步骤及自动审计接入要求。实施目标是：业务执行、命令回放和责任判断不再反查展示性审计；业务代码声明动作与必要事实，统一机制生成可供业务人员理解的操作记录。

状态：静态源码盘点完成；数据迁移、代码改造及运行验收均未执行。

## 1 适用范围与基线

1. 盘点完成日期为 2026-10-04。Git HEAD 为 `9c05531092210b45808f1bdc18cca5dd3d37590e`，事实依据包含当时工作区尚未提交的修改，不等同于该提交单独的内容。
2. 生产调用范围为 `backend/apps`、`backend/crates`。补充检查 `erp-client` 的审计展示及结果引用、`scripts` 和 `backend/scripts` 的集合访问。测试、示例、归档及审计历史证据目录不作为运行依赖；不得修改归档以完成本合同。
3. “依赖审计的业务”包括：读取审计恢复命令结果，使用审计字段进行业务校验或参与关系判断，以审计身份生成业务证据或关联来源。仅写入日志仍属于审计一致性要求，但不等同于反向业务依赖。
4. 下文 A、B 按命令或读取实现归组，不按 HTTP 路由数量计数；共享实现涵盖的多个动作必须全部迁移。C、D 可以与 A、B 指向同一业务，不得相加为业务总数。
5. 代码链接定位文件，方法名和行号定位本次基线；实施前必须按当前源码重新确认。文件移动不得成为遗漏条目的理由。
6. 本次只读取源码并编写文档；未启动服务，未连接 MongoDB/S3，未执行清库、迁移、E2E、编译或单元测试。静态可达调用链不证明真实环境的数据完整性、事务回滚或并发正确性。

### 1.1 集合读取覆盖

生产源码中直接链式调用审计集合读取接口的访问点共 34 处，已全部归入本合同；接口定义、测试及同一接口的间接业务调用不重复计入。

| 读取方式 | 访问点数 | 下游覆盖 |
| --- | ---: | --- |
| `audit_logs().find_by_id` | 25 | B01—B25 |
| `audit_logs().find_command_receipts_by_ids` | 4 | 通用回执 Service、工作流 adapter、回款提交、两类冲正提交；业务见 A01—A18 |
| `audit_logs().list_successful_by_resource` | 2 | 工作流 adapter 和库存调整撤回；业务见 C07、C08 |
| `audit_logs().list_work_item_creation_audits` | 1 | 创建人读取共享入口；业务见 C01—C06 |
| `audit_logs().search_logs` | 1 | 审计列表，见 E01 |
| `audit_events().search_audit_events` | 1 | 身份治理审计列表，见 E02 |

已补充检查原始集合名、集合常量、`AuditLog`/`AuditEvent` 类型、仓储别名、Port 实现、消息解析器及 `audit_id`/`audit_event_id` 用途。原始 `audit_events` 集合写入存在于角色授权仓储；未发现额外的生产反向业务读取。

### 1.2 当前关键事实

1. `audit_logs` 同时承载人类操作日志和机器命令协议。其 [ID 唯一索引](../backend/crates/erp-audit/src/indexes.rs) `uk_audit_logs_id` 也参与确定性回执的重复写入防护；不能只迁读取而删除这项防护。
2. 通用命令指纹保存在 `message` 的 `command_fingerprint=` 段；其他业务还使用 `command_sha256=`、分隔符结果及 JSON 信封，详见 §2.6。
3. 六类资金单据的创建人经审计进入 `ObjectFact.created_by`，随后参与工作项访问判断。五类实体已有 `created_by`，供应商付款实体没有该字段，详见 C01—C06。
4. `SeparationAuditFact` 及批量职责分离查询没有发现生产调用。不得将其列为已经运行的审计职责分离链路。
5. 审批命令和供应商接口连接的部分命令已经使用独立回执；不得将这些实现整体改回审计回执，见 §2.5。

## 2 依赖清单

### 2.1 A 类 通用命令回执依赖

共同链路为 [CommandReceiptServiceExt::committed_resource_id](../backend/crates/erp-audit/src/service.rs) → `find_command_receipts_by_ids` → [AuditLog::pick_committed_resource_id](../backend/crates/erp-audit/src/entity/audit_log.rs) → [CommandReceipt::match_fact](../backend/crates/application-core/src/command.rs)。读取审计 ID、操作人、动作、资源类型、成功标志、消息指纹及结果资源 ID。A04、A12、A13 直接在调用方 Executor 中批量查询；A17、A18 经工作流 Port 接入相同集合。

迁移要求：每项必须改为拥有领域的结构化命令回执；同一动作的首次查询、事务内查询、异常后的结果恢复均须同步迁移。返回“原结果快照”或“结果对象的当前视图”沿用各入口现有语义，不得统一改成一种。

| 编号 | 业务命令 | 已核实入口与读取位置 | 目标拥有领域及必须保留的结果 |
| --- | --- | --- | --- |
| A01 | 登记进项发票 `purchase_invoice_allocation.post` | [payable/invoice.rs](../backend/crates/erp-processes/src/finance_posting/payable/invoice.rs)，`register_purchase_invoice`，58、143 行 | `erp-finance`；发票 ID、原登记结果与分配事实 |
| A02 | 提交供应商付款 `supplier_payment.commit` | [payable/payment.rs](../backend/crates/erp-processes/src/finance_posting/payable/payment.rs)，`commit_supplier_payment_with_assets` / `committed_payment_view` | `erp-finance`；付款 ID、附件是否已提交的返回语义，禁止重复付款或分配 |
| A03 | 新建并提交客户回款 `customer_receipt.commit` | [customer_receipt.rs](../backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt.rs)，`replay_committed_receipt` / `recover_committed_receipt` | `erp-finance`；回款 ID及已提交结果 |
| A04 | 提交已有回款 `customer_receipt.submit` | [customer_receipt_submit.rs](../backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt_submit.rs)，`committed_submit` / `matches_committed_submit` | `erp-finance`；原单与原请求指纹；保留授权先行及同 Executor 查询 |
| A05 | 登记并过账销项发票 `invoice.commit` | [receivable/invoice.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice.rs)，`commit_invoice_with_assets` / `committed_invoice_view`；[invoice_attachments.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_attachments.rs) 构造身份 | `erp-finance`；发票 ID、附件归一化与历史指纹兼容 |
| A06 | 提交开票申请 `sales_invoice_request.submit` | [invoice_request/submit.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_request/submit.rs)，`submit_invoice_request` | `erp-finance`；申请 ID及绑定、启动结果 |
| A07 | 撤回开票申请 `sales_invoice_request.cancel` | [invoice_request/cancel.rs](../backend/crates/erp-processes/src/finance_posting/receivable/invoice_request/cancel.rs)，`cancel_invoice_request` | `erp-finance`；原申请撤回结果；历史身份将申请 ID 传入 `resource_type` 槽，必须保留专用兼容解码，禁止按新资源目录直接重算旧键 |
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

下表一行对应一处 `audit_logs().find_by_id` 读取实现，共 25 处。除表中专门说明外，均需迁入该命令拥有领域的回执集合，并保留其现有业务事实交叉验证。

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
| B11 | 采购提交 | [procure_to_pay/submission.rs](../backend/crates/erp-processes/src/procure_to_pay/submission.rs)，`replay_purchase_submit`，282 行 | 按当前/历史候选 ID 顺序读取 `PurchaseSubmitReceipt`；迁至 `erp-procurement`，保留审批、任务与版本结果，不能改变候选优先级 |
| B12 | 作废采购草稿 | [procure_to_pay/void_order.rs](../backend/crates/erp-processes/src/procure_to_pay/void_order.rs)，`replay_void_draft`，329 行 | `VoidDraftReceipt`；迁至 `erp-procurement`，继续核对当前作废状态及版本 |
| B13 | 商品维护责任交接 | [product_handover.rs](../backend/crates/erp-processes/src/product_handover.rs)，`replay`，150 行 | 指纹及当前商品责任视图；迁至 `erp-catalog`，保留目标资格、组织及当前访问控制 |
| B14 | 供应商责任交接 | [supplier_profile/handover.rs](../backend/crates/erp-processes/src/supplier_profile/handover.rs)，`replay_supplier_handover`，169 行 | 指纹后重读供应商当前责任；迁至 `erp-supplier` |
| B15 | 供应商能力责任交接 | 同文件 `replay_capability_handover`，196 行 | 指纹后重读能力负责人及版本；迁至 `erp-supplier`，不能与供应商账户交接共用不带目标类型的身份 |
| B16 | W26 供应商订单任务完成 | [supply_execution/complete.rs](../backend/crates/erp-processes/src/supply_execution/complete.rs)，`replay_task_completion`，160 行 | 完成收据包含终结动作 ID、任务/订单结果及处理结论；迁至 `erp-supply`，继续核对 `supplier_order_actions`，覆盖 D01 |
| B17 | 供应商履约订单跟进责任交接 | [supply_execution/handover.rs](../backend/crates/erp-processes/src/supply_execution/handover.rs)，`replay`，207 行 | 指纹及当前跟进人/版本；迁至 `erp-supply`，保留兼容的指纹后缀与当前权限重验 |
| B18 | W26 直接调查和任务内调查 | [supply_execution/investigate.rs](../backend/crates/erp-processes/src/supply_execution/investigate.rs)，`replay_investigation`，525 行 | 调查收据、证据 ID、操作号及任务版本；迁至 `erp-supply`，两个入口必须保持身份区分，覆盖 D01 |
| B19 | 供应商 API 能力配置变更 | [supplier_api/command.rs](../backend/crates/erp-processes/src/supply_governance/supplier_api/command.rs)，`update_capabilities`，226 行 | 确定性审计 ID 与指纹；迁至 `erp-supply` 独立回执，保留返回 `audit_event_id` 的关联合同；不能因其他连接命令已有回执而漏掉该入口 |
| B20 | 供应商结算差异正式决定 | [supply_settlement/difference.rs](../backend/crates/erp-processes/src/supply_settlement/difference.rs)，`replay_difference_decision`，121 行 | 操作号、结算单/差异版本及结果；迁至 `erp-supply`，保留差异、明细与单头联合核验 |
| B21 | 供应商结算草稿刷新 | [supply_settlement/draft.rs](../backend/crates/erp-processes/src/supply_settlement/draft.rs)，`replay_refresh`，254 行 | 请求号、来源快照、版本及明细/差异数量；迁至 `erp-supply`。来源未变化时仍写回执，不能用“无字段差异”省略此分支 |
| B22 | 供应商结算责任交接 | [supply_settlement/handover.rs](../backend/crates/erp-processes/src/supply_settlement/handover.rs)，`replay_handover`，180 行 | 指纹及当前制单人/组织/版本；迁至 `erp-supply` |
| B23 | 供应商结算差异处理人改派 | 同文件 `replay_handler`，199 行 | 指纹及当前差异处理人/版本；迁至 `erp-supply`，与结算责任交接保留不同动作身份 |
| B24 | 供应商结算提交财务复核 | [supply_settlement/review.rs](../backend/crates/erp-processes/src/supply_settlement/review.rs)，`replay_review_submission`，276 行 | 操作号、冻结主题/单头版本及工作项 ID；迁至 `erp-supply`，继续交叉验证任务和结算单 |
| B25 | 供应商结算复核决定 | 同文件 `replay_review_decision`，311 行；写入见 [review_posting.rs](../backend/crates/erp-processes/src/supply_settlement/review_posting.rs) | 复核结果、任务版本、结算版本及成本差额；迁至 `erp-supply`，保留原复核人校验、制单人分离规则及成本写序；结果不能由展示文案恢复 |

### 2.3 C 类 业务身份与取消终态依赖

C01—C06 的完整公共链路为：

`authority/funds/{payments,receipts}.rs` 或 `funds_document_brief/{payment,receipt}.rs` → [load_created_by_from_audit](../backend/crates/erp-read-models/src/workbench/authority/funds/query.rs) → `list_work_item_creation_audits` → [funds/mapping.rs](../backend/crates/erp-read-models/src/workbench/authority/funds/mapping.rs) → `ObjectFact.created_by` → [has_object_participation](../backend/crates/erp-workflow/src/service/work_item/access.rs)。最后一步判断创建人是否为当前账号，或账号是否参与根单据。它是参与关系条件，不能替代 RBAC、DataScope 或任务责任条件。

| 编号 | 业务对象 | 当前使用及来源 | 替代方案 |
| --- | --- | --- | --- |
| C01 | 客户回款 | 创建审计首个操作人进入资金权威事实和工作台简报 | 读取 [CustomerReceipt.created_by](../backend/crates/erp-finance/src/entity/receivable/customer_receipt.rs)，切换前核对字段与旧审计的差异 |
| C02 | 供应商付款 | 同上 | [SupplierPayment](../backend/crates/erp-finance/src/entity/payable/supplier_payment.rs) 当前没有创建人字段；新增不可变 `created_by`，新付款从已认证命令人写入，历史记录按 §5 回填 |
| C03 | 客户退款 | 同上 | 读取 [CustomerRefund.created_by](../backend/crates/erp-returns/src/entity/returns/customer_refund.rs)；不得改用可变化的当前经办人 |
| C04 | 供应商退款 | 同上 | 读取 [SupplierRefund.created_by](../backend/crates/erp-returns/src/entity/returns/supplier_refund.rs) |
| C05 | 回款冲正 | 同上 | 读取 [ReceiptReversal.created_by](../backend/crates/erp-returns/src/entity/returns/receipt_reversal.rs) |
| C06 | 付款冲正 | 同上 | 读取 [PaymentReversal.created_by](../backend/crates/erp-returns/src/entity/returns/payment_reversal.rs) |
| C07 | 库存调整撤回的原命令操作人 | [cancel_runtime.rs](../backend/crates/erp-processes/src/inventory_adjustment/cancel_runtime.rs)，`committed_cancel_actor`，238 行；按动作和消息中的实例前缀筛选，要求唯一原操作人 | 在 `erp-inventory` 保存撤回命令事实，包含调整单、审批实例、原操作人、命令身份、原因及结果引用；保留与现有审批回执的关联和回放身份校验 |
| C08 | 受阻审批取消结果恢复 | [cancel_blocked.rs](../backend/crates/erp-workflow/src/service/approval/execution/runtime_service/cancel_blocked.rs)，`load_cancel_blocked_terminal_facts`，236 行；从 `approval.cancel_blocked` 消息解析 `execution=… reason=…`，要求唯一审计并交叉验证执行、任务及实例终态 | 在 `erp-workflow` 保存结构化取消事实或扩展其拥有的 ERP 取消结果记录；保留 actor、execution、reason、blocker 与终态时间关系。已有 `ApprovalCommandReceipt.result_ref` 本身不足以替代全部事实 |

C01—C06 的两套消费入口必须同时迁移：[权威付款读取](../backend/crates/erp-read-models/src/workbench/authority/funds/payments.rs)、[权威回款读取](../backend/crates/erp-read-models/src/workbench/authority/funds/receipts.rs)、[付款简报](../backend/crates/erp-read-models/src/workbench/funds_document_brief/payment.rs)、[回款简报](../backend/crates/erp-read-models/src/workbench/funds_document_brief/receipt.rs)。不得只修展示而保留命令权威读取的审计依赖，也不得只修权威读取而使两套投影身份不同。

### 2.4 D 类 审计身份与业务关联耦合

下列路径不一定查询审计正文，但重建审计 ID 时会改变业务标识或来源关系，必须纳入迁移。

| 编号 | 场景 | 证据与当前行为 | 实施要求 |
| --- | --- | --- | --- |
| D01 | W26 调查及任务完成 | [supply_execution/investigate.rs](../backend/crates/erp-processes/src/supply_execution/investigate.rs)、[complete.rs](../backend/crates/erp-processes/src/supply_execution/complete.rs) 使用 `stable_evidence_id` / `stable_internal_idempotency_key`；算法在 [supplier_fulfillment/receipt.rs](../backend/crates/erp-supply/src/service/supplier_fulfillment/receipt.rs)，输入是确定性审计 ID | 改为由稳定命令身份派生证据与内部幂等键。兼容路径必须保持旧字节算法，既有动作和证据 ID 禁止重建 |
| D02 | 采购选源中的现有库存预占 | [sourcing_create/stock_posting.rs](../backend/crates/erp-processes/src/procure_to_pay/sourcing_create/stock_posting.rs)，`build_pending_line` 将 `audit_id` 写入 `StockReservationEntry.source_document_id` | 来源应明确为选源命令/分配事实。新增可判别的来源类型与命令引用或使用已有合适的正式来源；旧值保留并建立命令身份映射，禁止直接改写历史库存分录来源 |
| D03 | W29 受控关闭产生对账结论 | [work_item/close.rs](../backend/crates/erp-workflow/src/service/work_item/close.rs) 将当前回执 ID 传给领域关闭；[adapters/workflow/w29_close.rs](../backend/crates/erp-processes/src/adapters/workflow/w29_close.rs) 用该 ID 的指纹生成 `w29-close-…` 结论 ID | A18 拆分后仍沿用稳定命令身份；不可将随机新审计 ID 用于结论去重。原结论序号、关闭类型及证据引用保持不变 |

### 2.5 E 类 正常审计用途与已有独立机制

| 编号 | 场景 | 核实结论及处理要求 |
| --- | --- | --- |
| E01 | 系统操作日志列表 | [admin/audit_log.rs](../backend/apps/web-api/src/core/handler/admin/audit_log.rs) → `AuditLogService::audit_log_list` → `search_logs`；属于合法审计查询。后续增加中文及结构化响应，保留旧响应字段的兼容窗口 |
| E02 | 身份治理审计列表 | [access_control/mod.rs](../backend/crates/erp-identity/src/service/access_control/mod.rs) 的 `audit_event_list` → `audit_events.search_audit_events`；前端 [access-audit](../erp-client/features/access-audit) 读取该语义。动作注册表归身份域，不得把业务日志无条件加入其选项 |
| E03 | 供应商连接通用治理命令 | [supplier_api/command.rs](../backend/crates/erp-processes/src/supply_governance/supplier_api/command.rs) 的 `replay_command` 读取 `supplier_api.command_receipt`；[SupplierConnectionCommandReceipt](../backend/crates/erp-supply/src/entity/supplier_api/governance/receipt.rs) 已存独立指纹和结果。`audit_event_id` 是关联引用，保留即可；B19 是另一个仍依赖审计的入口 |
| E04 | 审批命令 | [ApprovalCommandReceipt](../backend/crates/bpm/src/model/command_receipt.rs) 已独立建模，由 `erp-workflow` 的 `approval_command_receipts` 集合和 [唯一索引](../backend/crates/erp-workflow/src/indexes/bpm.rs) 持久化。继续复用；C08 另有审计终态依赖，不能据此核销 |
| E05 | 供给创建、修订、可供性修改 | [offering/commit.rs](../backend/crates/erp-processes/src/supply_governance/offering/commit.rs) 先持久化领域命令再写同事务审计；领域实现位于 [supplier_offering](../backend/crates/erp-supply/src/service/supplier_offering)。不属于查日志恢复命令；A16、B04 单列 |
| E06 | 供应商结算草稿创建 | [supply_settlement/draft.rs](../backend/crates/erp-processes/src/supply_settlement/draft.rs) 先走 `prepare_scoped_statement` 的领域重放分支；虽然新写审计包含机器格式结果，未发现读取该创建审计的生产分支。不得与 B21 的刷新混淆；自动化时清除新事件中不再需要的机器协议，历史内容保持 |
| E07 | API 返回的审计编号 | 供应商 API 结果的 `audit_event_id`、导入结果中的审计编号是对外关联合同。前端 [supplier-api commands](../erp-client/features/supplier-api-connections/api/commands.ts) 映射为 `auditEventId`。回执解耦不授权删除或重命名这些响应字段；它们不等同于额外的审计正文反查 |
| E08 | 登录、敏感读取与普通业务成功审计 | 登录在 [auth/login.rs](../backend/apps/web-api/src/core/handler/auth/login.rs) 采用 best-effort；敏感读取及正式写入沿各自合同保留。不能统一改成所有失败可忽略，也不能把登录强行加入业务写事务 |

### 2.6 历史消息协议及兼容入口

| 协议 | 生产消费方 | 兼容要求 |
| --- | --- | --- |
| `command_fingerprint=…` 及其历史兼容格式 | A 类，[application-core/command.rs](../backend/crates/application-core/src/command.rs) | 保留当前/历史候选 ID、原指纹算法、资源和 actor 校验及损坏分类；迁入独立结构化指纹字段 |
| `command_sha256=…`，可能允许说明后缀 | 销售建单/提交/交接、商品/供给/供应商/能力/履约责任交接、供应商能力修改 | 每种 parser 的匹配规则分别保留，不能用宽松“包含摘要”统一替代；商品、供给及部分交接身份含历史原始键拼接，转换必须兼容旧定位 |
| 采购版本化回执信封 | B08—B12，[purchase_order/command_receipt.rs](../backend/crates/erp-procurement/src/entity/purchase_order/command_receipt.rs) | 迁移保留强类型 payload、schema 版本、错误分类及旧候选顺序；不是仅复制指纹和结果 ID |
| `command_sha256=…;result=…` | 历史导入业务确认、供应商结算 | 按各自现有解析器恢复字段与枚举；任务版本、批次/单据版本和结果数量不得丢失 |
| `command_sha256=…;execution=…` | 历史导入执行，[import_apply/execution.rs](../backend/crates/erp-processes/src/import_apply/execution.rs) | 保留动作、批次/试算/后台任务版本及状态、影响项数和下一步；不得套用确认结果的 parser |
| `fp=…;e=…;o=…;t=…` / `fp=…;a=…;o=…;t=…;r=…` | W26 调查/任务完成，[supply_execution/receipt.rs](../backend/crates/erp-processes/src/supply_execution/receipt.rs) | 保留证据或终结动作、订单/任务版本、结论，以及重复字段拒绝规则 |
| JSON `ReceiptEnvelope<T>` | B03，[task_decision/guard.rs](../backend/crates/erp-processes/src/integration_resolution/task_decision/guard.rs) | 保留完整强类型结果及身份检查，不得把 JSON 正文转成中文后继续解析 |
| `execution=… reason=…`；库存撤回实例前缀 | C08、C07 | 专用一次性迁移解码；校验实例、执行、原操作人及唯一性，禁止只解析文本即认定命令成功 |

新回执写入不得使用审计 `message` 承载机器协议。旧协议解析代码只允许存在于有退出条件的兼容模块及迁移工具中。

### 2.7 未发现生产调用的接口

以下接口位于 [erp-audit/repository/audit_log.rs](../backend/crates/erp-audit/src/repository/audit_log.rs)。按当前工作区全仓搜索，除定义、实现、导出及测试外，未发现生产调用。

| 接口 | 处理要求 |
| --- | --- |
| `list_separation_facts_by_resources` / `SeparationAuditFact` | 不登记为现行职责分离依赖；删除前复核 workspace 调用和库检查，不得为证明其必要性重新接入业务规则 |
| `list_successful_work_item_fact_audits` | 不与实际使用中的 `list_work_item_creation_audits` 混淆；移除无调用入口或按明确新需求另行登记 |
| `list_master_mapping_task_history` | 不登记为当前映射业务依赖 |
| `list_master_mapping_task_histories` | 同上；不得仅凭“时间线”注释声称页面已接入 |

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
| 历史兼容 | 原回执 ID或可验证的别名映射、原协议版本及来源；未保存原始幂等键时不得尝试反推它 |

同步数据库命令的成功回执与业务事实同事务提交。外部任务已有的 `Processing`、`Rejected`、`Unknown` 生命周期由拥有领域继续解释，不得套用“HTTP 成功即命令最终成功”。回执保存期限必须覆盖约定的重试与追溯期限，不得沿日志归档策略删除仍承担去重作用的回执。

### 3.3 身份与终态事实

1. `created_by` 记录不可变创建人；责任交接不得覆盖它。`handled_by`、审批决定人、当前责任人、付款执行人等字段各自保留语义，禁止互相替代。
2. C01、C03—C06 优先使用已有字段。出现字段与审计不一致、空值、多名候选时，必须输出异常项并停止该记录的自动切换；不得取任意第一人或当前账号补齐。
3. C02 的新字段必须兼容旧文档读取，并同时安排旧数据回填及切换守卫；缺失身份不得自动授予参与资格。
4. C07、C08 保存不可变、结构化的取消操作事实。原回执、目标、操作者、原因、执行节点及终态必须相互验证；不能将终态校验缩减为“有一条回执”。
5. 静态权限、DataScope、对象参与关系、当前任务责任、审批规则分别执行。回执命中不能成为绕过这些校验的通行证；迁移不得改变既有错误顺序和不可见对象的响应口径。

## 4 自动审计执行合同

### 4.1 结构化事件

审计事件必须保存稳定动作代码、动作/schema 版本、操作人身份与安全名称快照、目标类型/ID/业务编号快照、业务结果、允许记录的字段变化、关联命令/请求以及发生时间。旧 `AuditLogItem` 字段在兼容期保留，新增字段必须兼容历史读取。

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
7. 注册入口必须明确 `Audited` 或带原因的免记策略。门禁检查入口覆盖、注册及依赖边界；不能以文本命中数宣称所有写操作都已审计。
8. HTTP 中间件可收集请求关联、耗时和拒绝信息；不得独自承担成功业务审计，因为后台任务、CLI、重放和跨域事务具有独立边界。
9. 权限拒绝与执行失败记录使用独立尝试事件语义；回滚不能抹掉已要求保留的拒绝记录。`OutcomeUnknown` 记录未知并保留原命令查证入口，不得误标为失败或自动重放。
10. 保留身份策略事务的提交后刷新、未知结果失败关闭及请求取消收尾合同。普通 `run_audited` 与身份策略事务必须分别评估生命周期，禁止用一个通用包装静默替换全部特殊行为。

### 4.3 展示与查询

1. 对业务人员展示中文动作、操作人、业务编号、结果及字段变化；技术动作代码和追踪号置于详情。
2. 以结构化字段筛选和导出，不从自由文本解析业务状态。日志模板变化不得影响业务执行或回放。
3. 审计记录的可见性及关联对象链接执行相应查询授权；取得审计列表权限不自动取得银行信息、客户敏感资料或关联单据的读取资格。
4. 查询和模板渲染可以扩展，成功事件的最低事实仍须与业务事务一致；异步展示投影失败不能丢失原始审计事实。

## 5 迁移及回退合同

### 5.1 执行顺序

| 阶段 | 必须完成的工作 | 退出条件 |
| --- | --- | --- |
| M0 固定合同 | 为 A01—A18、B01—B25、C01—C08、D01—D03 建立逐项实施状态；确认字段、唯一索引、结果语义、历史解码与回退版本 | 每项具备拥有领域、迁移映射及验收用例；不得用全局“已完成”代替逐项状态 |
| M1 建立兼容版本 | 新增独立回执/缺失事实及索引；先上线兼容读写版本；新旧数据同事务写入 | 新旧 writer 共存期间仍共享原有唯一防护，任一版本重试不会重复产生业务效果 |
| M2 回填与核对 | 按动作、旧协议及稳定游标分批回填；记录高水位并处理增量；核对新旧回执和身份结果 | 所有目标记录已迁移或进入明确的阻断清单；异载荷、损坏、缺业务引用不得当作未执行命令 |
| M3 切换业务读取 | 按领域逐项切到新回执和领域事实；先对比新旧结果，再停用旧读取 | 命令恢复、当前授权、对象参与关系、任务及版本结果一致；旧回退版本仍可识别兼容期命令 |
| M4 接入自动审计 | 迁移后的领域接入类型化动作及统一执行包装，交付中文查询与展示 | 业务函数不再手工拼装机器日志；成功原子性及失败分类符合 §4 |
| M5 结束兼容 | 停止旧 writer，确认回退窗口结束、回填无缺口；删除生产旧日志 parser 和无调用查询 | 审计仅剩正常写入和展示读取；历史日志保留；门禁阻止新增业务反查 |

M1 的兼容版本必须在同一事务内写新回执和旧版本仍需的兼容记录；成功事件可在既有记录上兼容扩展或独立关联，但查询不得把技术兼容回执重复显示成第二次业务操作。只做“新版本读新表、旧版本继续写旧日志”的滚动发布不符合本合同。

M2 回填只能复制已证明的已提交结果，禁止重跑原业务写入、重新过账、重新启动审批或调用供应商。旧 ID 无法恢复原始幂等键时，保留稳定别名及版本化定位规则；不得生成一个与原请求无关的新键后宣称完成迁移。

M3 兼容读取必须有动作范围、监测指标及删除条件。损坏或新旧不一致必须明确失败，不得通过回退到旧记录掩盖新回执冲突。切换完成后，运行时不得永久双读审计。

M5 以后禁止回退到仍仅识别旧日志协议的二进制。确需回退时，使用能够读取新回执的兼容版本；不能删除新回执或撤销已经发生的业务事实来恢复旧代码。

### 5.2 分批实施顺序

| 批次 | 范围 | 必须交付的结果 |
| --- | --- | --- |
| P1 基础与样板 | A01、C01 及服务履约确认 | 独立财务回执样板、创建人领域读取样板、自动审计及中文事件样板；服务履约确认本身没有本清单内的日志回放依赖，不得凭空增加业务重试语义 |
| P2 资金与逆向 | A02—A13、C02—C06 | 完整资金回执及六类创建人投影解耦；覆盖附件和精确原版本指纹 |
| P3 销售采购与主数据交接 | B04—B15、D02 | 销售/采购版本化结果、批量选源及各类交接；保留库存分录来源兼容 |
| P4 供应链治理与结算 | A16、B16—B25、D01 | W26/W20/结算回执解耦；不重复改造 E03 已有独立回执 |
| P5 工作流履约与集成导入 | A14、A15、A17、A18、B01—B03、C07、C08、D03 | 任务/取消终态、验收、导入和 W29 全部解耦；保留后台与外部任务生命周期 |
| P6 收口 | E 类保留边界、§2.7 遗留接口、兼容退出及所有事件目录 | 删除业务反查入口，统一安全中文展示，执行依赖门禁及文档同步 |

每批必须完成本批 M0—M4 和相应验收后再核销。M5 必须等待该批兼容窗口满足条件；其他已独立领域的自动审计接入不必等待全项目 M5。

## 6 验收合同

### 6.1 必须覆盖的行为

| 编号 | 验收项 | 通过标准 |
| --- | --- | --- |
| V01 | 同键同载荷 | 返回原命令允许的结果形态；业务写入、资金流水、任务、附件及成功审计均不重复 |
| V02 | 同键异载荷 | 保留稳定冲突及原错误优先级；不覆盖旧回执，不退回审计掩盖冲突 |
| V03 | 旧版本重试 | 旧候选 ID、原指纹、专用结果和 A07 异常身份槽均按兼容合同识别 |
| V04 | 同事务与失败停止 | 正式业务事实、回执、审计使用同一个 Executor；任一步失败停止后续步骤；编排单测不得被描述为真实数据库回滚证明 |
| V05 | 并发提交与唯一性 | 新旧 writer 或并发新 writer 对同一命令只形成一份有效业务结果；依赖唯一约束，不以“先查后写”代替并发防护 |
| V06 | 未知提交结果 | 只查证原命令；恢复失败保持未知及原错误，不自动执行第二次写事务 |
| V07 | 身份与授权 | 六类创建人由领域事实形成；切换前后可见集合、参与资格及允许动作对等；RBAC、DataScope、任务责任分别保持 |
| V08 | 取消事实 | 缺失、重复或不匹配的 actor/实例/执行/原因/终态必须拒绝；C07、C08 不因存在回执就通过 |
| V09 | 专用结果 | 采购/结算/导入/W26/W29 的原版本、任务、证据、成本差额和结果状态无损迁移；覆盖 B21 无变化分支 |
| V10 | 标识与关联 | D01—D03 的旧业务 ID、库存来源和结论证据不改变；API 审计编号保持可关联 |
| V11 | 展示与隐私 | 用中文说明人物、对象、动作和结果；业务字段名明确；秘密与完整敏感值不落日志；历史未知字段不伪造 |
| V12 | 回填与回退 | 回填可断点续跑且不触发业务动作；新旧数据可核对；兼容期旧版本可读，结束后禁止回退到不兼容版本 |
| V13 | 依赖退出 | A、B、C 的生产业务读取不再访问审计；D 改为稳定命令来源；保留的审计查询仅为授权展示/追溯用途 |
| V14 | 自动记录覆盖 | 新增/迁移写用例必须登记事件或明确免记策略；HTTP、后台任务及内部用例均经过对应边界；多对象事件无重复遗漏 |

### 6.2 验证证据边界

1. 实现阶段遵守 [backend/AGENTS.md](../backend/AGENTS.md)：只新增/执行内联库单元测试，不新增、修改或执行集成测试、真实 Mongo/S3 验收例程。
2. 单元测试必须执行真实生产编排和解析逻辑，覆盖成功、失败、边界、回放零写入及同 Executor 传递；禁止复制实现后测试副本，禁止用源码字符串计数替代业务行为验证。
3. 文档本身只执行路径/内容核对与 `git diff --check`；不得因本合同已落盘声称 V01—V14 已通过。
4. V05、真实事务回滚、历史数据回填完整性、滚动版本兼容及最终中文浏览器体验需要独立运行验收记录。当前静态盘点不核销这些项目，也不执行真实环境操作。
5. 后续每批代码交付执行受影响 crate 的 fmt/check/库单测/clippy；公开签名、领域边界、BPM 和权限生成变动按仓库要求追加门禁。提交前执行仓库规定的全量质量门禁。

## 7 文档与源码差异处理

以下差异必须在实施对应批次时同步修订，不能静默选用其中一边。

| 文档陈述 | 当前源码事实 | 执行要求 |
| --- | --- | --- |
| [backend/docs/audit-logging.md](../backend/docs/audit-logging.md) 写有“项目尚未提供跨请求幂等键” | A、B 多条现行命令已经提供跨请求身份与回执 | 改为明确描述对应管理对象的局部范围，或更新全项目回执合同；不得用该旧表述否定现行幂等要求 |
| 同文提到 `run_audited_transaction` 及两条路径均由独立 Tokio 任务持有 | 当前通用 [run_audited](../backend/crates/erp-processes/src/audit/transaction.rs) 直接等待 `with_transaction`，该函数没有创建独立任务 | 逐路径核对请求取消与任务所有权。需独立收尾时显式实现并验证；本次静态盘点不承诺普通事务具有该特性 |
| 同文要求 `AuditLog` 全部在事务开始前校验 | 自动审计的部分事实只能在业务执行后确定；现行服务履约确认也在事务步骤内构造审计 | 将约束分为事务前元数据校验和提交前结果事实校验，同时保留失败原子性 |
| [query-selector-decoupling-contract.md §11.4](query-selector-decoupling-contract.md) 限定身份域审计事件目录 | 业务 `audit_logs` 是不同集合与语义 | 自动中文事件目录不得无条件复用身份审计候选；分别登记后由明确查询合同聚合 |

## 8 完成条件

1. A01—A18、B01—B25、C01—C08、D01—D03 均具有实施及验收证据；E 类合法用途与既有独立机制保留正确边界。
2. 审计展示文本、事件中文模板和审计归档策略的变化，不再影响命令去重、业务状态、原操作人识别或参与资格。
3. 普通业务函数不再重复构造 `AuditLog` 和调用审计仓储；必要的业务语义以类型化事实明确提供，由统一边界记录。
4. 新旧版本兼容、历史异常记录处理和回退限制均已核销，生产旧协议解析器退出；历史审计不被删除或改写。
5. 未取得真实环境证据的验收项继续标为未验证，禁止以编译、单元测试、静态扫描或文档完成替代运行验收。
