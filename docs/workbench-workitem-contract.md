# 工作台 WorkItem 全覆盖与原地处理合同

版本：1.0  
状态：生效  
适用页面：W01 我的工作台  
适用范围：全部审批运行时责任与全部已注册非审批人工责任

## 1. 目标与适用边界

1. 系统中每一项需要具体人员继续推进业务的开放责任，必须以唯一 `OPEN WorkItem` 进入 W01。
2. W01 必须是审批和任务的唯一处理入口。当前责任人必须能够在右侧详情原地查看充分事实并提交正式领域命令。
3. `WorkItem` 只投影“当前由谁负责什么”。审批实例、业务单据、不可变来源证据、领域状态和命令收据仍由各自领域拥有，不得复制进 `WorkItem` 作为第二事实源。
4. 草稿、系统自动门禁、只读预警和没有已签署人工动作的异常不构成任务，不得为满足队列数量而创建可被通用完成的空壳 `WorkItem`。
5. 文档一旦规定某状态必须由人员执行，实施必须同时签署生产规则、唯一责任解析、业务对象版本、原地作业面、正式完成命令和终态语义；任一项缺失时不得上线该人工状态。

## 2. 开放责任形成规则

每个任务生产者必须在同一业务事务内完成以下操作：

1. 写入或确认权威业务事实；
2. 解析唯一 `owner_user_id`、`owner_role` 与 `owner_organization_id`；
3. 冻结 `business_object_type`、`business_object_id` 与 `subject_version`；
4. 创建或复用唯一开放 `WorkItem`；
5. 写入业务可读的 `reason_code`、`impact_summary`、优先级和截止时间；
6. 写入审计记录；
7. 在无法解析唯一合格责任人时整体失败关闭，不得创建团队池、空责任人或创建人兜底任务。

同一责任键不得存在两条开放任务。业务事实已经完成、作废、冲正或失效时，生产者必须在同一事务内完成或关闭对应任务。

## 3. 活跃任务登记表

下表是 W01 唯一允许返回的活跃任务类型。未列组合不得创建、路由或展示。

| `work_item_type` | `business_object_type` | 生产时点与责任 | W01 原地作业面 | 唯一正式完成语义 |
| --- | --- | --- | --- | --- |
| `DOCUMENT_APPROVAL` | `sales_order`、`voucher_sales_order`、`sales_change_order`、`purchase_order`、`purchase_change_order`、`stock_adjustment`、`customer_receipt`、`customer_refund`、`supplier_refund`、`receipt_reversal`、`payment_reversal` | 审批运行时激活单人节点时，责任人为冻结的当前实例审批人 | 通用审批详情与决定栏 | `submit_decision(APPROVE\|REJECT)` 完成当前节点任务并由审批运行时推进 |
| `PROCUREMENT_ORDER_CREATION` | `sales_order` | 销售单最终生效且存在待分配供给时，由采购责任规则解析具体采购人员 | 供给分配与采购单创建面 | 创建库存预留并对缺口创建采购单；全部分配成功后完成任务 |
| `FULFILLMENT_OPERATION` | `purchase_receipt`、`delivery`、`electronic_delivery`、`service_fulfillment` | 对应履约草稿形成时，由履约对象责任规则解析具体人员 | 收货、仓发、电子交付或服务履约面 | 对应强类型过账或确认命令形成正式履约事实后完成任务 |
| `CUSTOMER_ACCEPTANCE_REGISTRATION` | `sales_order` | 发货或交付形成待验数量时，由销售单负责人承担 | 客户验收登记面 | `post_customer_acceptance`；待验数量清零时完成任务，冲正重新形成待验时重建任务 |
| `SUPPLIER_PAYMENT_EXECUTION` | `payable_account` | 采购单最终通过并形成开放应付时，由付款责任规则解析具体财务人员 | 应付核销与付款登记面 | 付款登记、核销和过账同事务执行；开放应付清零时完成任务，分次付款时保持开放 |
| `SALES_INVOICE_EXECUTION` | `receivable_account` | 应收子账形成可开票余额时，由开票责任规则解析具体财务人员 | 销项开票登记面 | `post_invoice`；可开票余额清零时完成任务，部分开票时保持开放 |
| `CARD_FUNDS_REVIEW` | `receivable_account` | 卡券销售首次生效并原子写入 `OpeningPending` 应收时，由 `CARD_FUNDS_REVIEW` 财务责任规则解析具体人员 | 卡券票款期初复核面 | `complete_card_funds_review` 提交期初复核决定并完成当前任务；驳回必须在同一事务创建同类型开放后继任务 |
| `CARD_FUNDS_DELTA_REVIEW` | `receivable_account` | 已完成上一轮复核的卡券销售变更形成非零应收差额，并原子写入 `SyncDeltaPending` 时，由 `CARD_FUNDS_REVIEW` 财务责任规则解析具体人员 | 卡券票款差异复核面 | `complete_card_funds_review` 提交差异复核决定并完成当前任务；驳回必须在同一事务创建同类型开放后继任务 |
| `SUPPLIER_SETTLEMENT_REVIEW` | `supplier_settlement_statement` | 结算单提交复核时，由结算责任规则解析具体人员 | 供应商结算复核面 | 结算确认或驳回强类型决定形成终态并完成任务 |
| `IMPORT_BUSINESS_CONFIRMATION` | `LEGACY_IMPORT_BATCH` | 导入试算生成销售、采购、运营、仓库或财务确认范围时，按固定范围角色解析具体人员 | 导入批次范围确认面 | `CONFIRM_SCOPE` 或 `RETURN_FOR_FIX` 完成当前范围任务 |
| `INTEGRATION_RESULT_UNKNOWN` | `integration_error_task`、`reconciliation_difference` | 外部结果不可判定且需要人工核实时，由集成责任规则解析具体人员 | 集成异常与对账差异处理面 | W29 强类型任务决定达到终态后完成任务 |
| `BUSINESS_EXCEPTION` | `integration_error_task`、`reconciliation_difference` | 集成业务校验或对账出现需人工处置的确定异常时，由集成责任规则解析具体人员 | 集成异常与对账差异处理面 | W29 强类型任务决定达到终态后完成任务 |
| `INTEGRATION_RESULT_UNKNOWN` 或 `BUSINESS_EXCEPTION` | `SUPPLIER_FULFILLMENT_ORDER` | 供应商履约结果未知或确定失败时，由供应商履约责任规则解析具体人员 | 供应商履约调查面 | `complete_order_task`；必须先存在可验证终态结果与供应商动作证据 |
| `BUSINESS_EXCEPTION` | `MASTER_MAPPING_TASK` | 商城同步无法唯一映射主数据时，由主数据责任规则解析具体人员 | 主数据候选映射确认面 | `confirm_mapping_task` 固定目标并完成任务 |
| `BUSINESS_EXCEPTION` | `SUPPLIER_OFFERING` | 供应商停止供给并形成不可变安全暂停时，由安全暂停规则创建唯一后续任务 | 供应停止影响核对面 | `complete_supply_exception_task` 登记证据并完成核对任务；供给和发布安全暂停继续生效 |

## 4. 单据审批全覆盖

1. `PROCESS_REQUIRED` 的 11 个单据类型必须全部通过 `DOCUMENT_APPROVAL` 投影当前活动审批节点：`SalesOrder`、`VoucherSalesOrder`、`SalesChangeOrder`、`PurchaseOrder`、`PurchaseChangeOrder`、`StockAdjustment`、`CustomerReceipt`、`CustomerRefund`、`SupplierRefund`、`ReceiptReversal`、`PaymentReversal`。
2. `NO_APPROVAL` 的 9 个单据类型不得创建审批实例或审批任务：`SupplierPayment`、`PurchaseReceipt`、`Delivery`、`ElectronicDelivery`、`ServiceFulfillment`、`CustomerAcceptance`、`Invoice`、`SalesReturnCase`、`PurchaseReturnOrder`。
3. `NO_APPROVAL` 不等于“不进入工作台”。其中已签署人工执行合同的付款、四类履约、客户验收和发票必须按第 3 节创建对应非审批任务。
4. `SalesReturnCase` 与 `PurchaseReturnOrder` 当前仅有草稿创建事实，没有已签署的提交转换、唯一责任规则和正式完成命令；草稿不得进入 W01。新增人工执行状态前必须先补齐第 1 节第 5 条的完整合同。

## 5. 原地处理合同

1. W01 必须按 `work_item_type + business_object_type` 命中显式处理器，不得使用默认处理器。
2. 右侧详情必须直接嵌入对应业务作业面；嵌入模式必须锁定当前 `work_item_id`、业务对象、`subject_version`、任务版本和必要路由上下文，不得让用户切换到队列中的其它对象。
3. 所有写命令必须携带任务身份、期望任务版本、业务对象身份、期望业务版本和幂等键；涉及敏感收款账户、映射候选、外部动作证据或安全暂停证据时还必须携带页面已核对事实的 ID 与版本。
4. Service 必须在命令事务内重验当前责任人、权限快照、任务开放状态、对象身份、冻结版本、岗位分离和领域前置条件。
5. 成功结果必须由正式领域命令完成、关闭或保持任务开放。前端不得先删除队列行，也不得调用通用 `complete` 补结束状态。
6. 任务完成后 W01 必须使服务端队列失效、宣布完成结果并选择下一条；命令失败或结果未知时必须保留当前任务与用户输入。
7. 「打开原单据」只允许作为补充查阅动作。工作台原地处理面缺失时必须显示不可处理错误并停止提交，不得把跳转当作完成能力。
8. 正式决定完成当前任务但领域仍保持待处理状态时，必须在同一事务创建已登记处理器的开放后继任务；不得返回“线下协作”“后继待配置”或其它脱离 W01 的结果。W13 驳回适用本条。

## 6. 统一信息合同

### 6.1 队列信息

每个队列条目必须展示：

1. 业务可读任务类型；
2. 稳定单号或业务对象标题，不得展示裸 MongoDB ID、UUID 或内部 handler key；
3. 往来方、来源单号或服务端 `list_summary` 中至少一项可辨识业务摘要；
4. 关键金额；没有金额时不得伪造零金额；
5. 受阻、超期或到期状态；
6. 当前选中状态。

### 6.2 统一任务上下文

每个审批和非审批任务详情顶部必须按相同顺序展示：

1. 任务类型、状态与优先级；
2. 为什么到当前责任人；
3. 不处理会阻断的业务结果；
4. 当前应执行的下一动作；
5. 责任角色、责任人和责任组织；
6. 进入工作台时间与截止时间；
7. 全部当前动作阻塞原因。

上述字段必须来自服务端 WorkItem 投影。前端只允许做格式化和受控文案映射，不得根据页面状态推断责任、原因或影响。

### 6.3 强类型业务事实

原地作业面还必须按任务种类展示下列事实：

| 任务族 | 必须展示的业务事实 |
| --- | --- |
| 审批 | 单据编号、往来方、金额、关键行、提交流程版本、当前轮次、当前节点、当前审批人、最近驳回人与原因 |
| 供给分配 | 销售单与行、需求数量、可用供给来源、目标仓库、预计交期、将形成的库存预留和采购单 |
| 履约与验收 | 来源销售或采购单、履约类型、往来方、行与数量、已履约/待履约或已验收/待验收数量、过账结果 |
| 付款 | 采购来源、供应商、应付总额、开放余额、付款条件、到期日、默认收款户名、开户行、脱敏账号及账户版本 |
| 开票与票款 | 销售来源、客户、应收或可开票余额、已开票额、复核种类、期初或差异依据及决定结果 |
| 供应商结算 | 供应商、结算期间、汇总金额、差异、证据状态、当前可执行决定 |
| 导入确认 | 批次、环境、批次版本、试算版本、确认范围、行数与错误摘要、返回修正原因 |
| 集成与供应商履约异常 | 来源系统、来源事件或订单、冻结版本、最后动作、可验证外部结果、证据与终态结论 |
| 主数据映射 | 来源快照、外部身份、候选目标、候选资格、证据说明和确认目标 |
| 供应停止 | 供应商供给、停供来源版本、不可变安全暂停操作、受影响发布、处置证据、核对结论及“完成不解除暂停”边界 |

## 7. 失败关闭与退役类型

1. 服务端活跃任务注册表只允许第 3 节所列 12 个 `WorkItemType`。
2. 下列类型已经退役：`PURCHASE_ORDER_REVIEW`、`SALES_CHANGE_IMPACT_REVIEW`、`SALES_CHANGE_FINANCE_REVIEW`、`OWNERSHIP_MIGRATION_SALES_CONFIRMATION`、`OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION`、`INVENTORY_ADJUSTMENT_REVIEW`、`FINANCE_CORRECTION_REVIEW`。
3. 退役类型只允许用于旧数据反序列化；新写路径、队列筛选、处理器路由、前端元数据和正式命令不得接受这些类型。
4. 采购单审批决定必须且只能提交至 `POST /admin/approval-decisions`；`POST /admin/purchase-orders/{id}/review-decisions` 路由、`purchase_order:review` 权限和采购单详情审核入口必须不存在。
5. 上线前发现退役类型的开放记录时必须通过受控 reset 或专用迁移清除，不得转换成其它任务类型，不得使用通用完成接口掩盖。
6. 未登记类型与对象组合必须返回稳定错误 `WORK_ITEM_HANDLER_UNMAPPED`；退役类型必须返回 `WORK_ITEM_TYPE_RETIRED`；导入责任角色无法映射固定确认范围时必须返回 `IMPORT_CONFIRMATION_SCOPE_UNMAPPED`。
7. 前端遇到服务端合同之外的任务时必须显示“任务处理器未登记”并停止提供处理动作。

## 8. 验收门禁

上线前必须逐项满足：

- [ ] 11 个 `PROCESS_REQUIRED` 单据逐一启动审批，并验证当前活动节点只形成一条指定到人的 `DOCUMENT_APPROVAL`；
- [ ] 第 3 节每个允许组合均由真实业务动作创建任务，任务可在 W01 打开并提交正式命令；
- [ ] 原地命令成功后，任务终态、业务事实、审计与队列刷新一致；部分付款、部分开票、部分验收等非终态场景保持任务开放；
- [ ] 卡券首次生效与非零销售变更差额分别原子形成期初/差额复核任务；W13 驳回完成当前任务并原子形成同类型开放后继任务；
- [ ] 所有详情均展示第 6.2 节统一任务上下文，并满足第 6.3 节相应业务事实；
- [ ] 当前责任人、无权限人员、已转交人员、版本冲突、重复幂等键与对象不匹配场景均按合同重验；
- [ ] 退役类型、未知对象组合和未知导入责任角色均失败关闭；
- [ ] 采购审批只能从 `DOCUMENT_APPROVAL` 提交统一决定，采购单旧审核路由、权限、对象投影和页面动作均不存在；
- [ ] 仓发、电子交付、服务履约和供应商直发完成后，客户验收任务按待验数量形成、保持、完成或重建；
- [ ] 供应停止任务完成后，安全暂停操作、暂停修订和发布暂停状态全部保持不变；
- [ ] `/workspace/tasks` 只重定向到 `/workspace`，不存在第二个待办入口或团队领取入口；
- [ ] 浏览器宽屏与窄屏均验证列表可扫读、详情信息完整、确认弹窗可操作、成功后连续选择下一条。
