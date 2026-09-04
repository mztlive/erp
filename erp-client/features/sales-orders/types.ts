import type { StatusTone } from "@/components/ui/status-badge"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

export type SalesOrderNature = "physical_service" | "card_voucher"
export type SalesOrderOrigin = "erp" | "mall"

export type SalesOrderCreateIntent = "SAVE_DRAFT" | "SUBMIT"

export type SalesLineProcurementResponsibility = {
    rowKey: string
    resolved: boolean
    ownerUserId?: string
    ownerName?: string
    matchedRuleType?: string
}

export type SalesOrderDraftLineInput = {
    rowKey: string
    name: string
    sku: string
    /** 公司商品池返回并由销售单锁定的精确 SKU 修订 ID。 */
    skuRevisionId: string
    /** 采购责任解析使用的服务区域；空值表示不按区域限定。 */
    serviceRegion?: string
    quantity: string
    unit: string
    unitPriceGross: string
    dueDate: string
    faceValue: string
    giftRate: string
    cardForm: string
}

/** M5 建单仅引用已有有效合同的当前修订；新合同先经 W04 上传 Dialog 归档。 */
export type SalesOrderContractInput = {
    contractId: string
    requestedContractRevisionId: string
}

/** M5 建单输入；合同必须选择已有有效版本。 */
export type CreateSalesOrderInput = {
    /** 页面生命周期内冻结；结果未知时与原幂等键一起重用。 */
    orderNo: string
    contract: SalesOrderContractInput
    nature: SalesOrderNature
    /** 负责销售用户 id（当前登录用户）。 */
    ownerUserId: string
    /** 负责销售展示名。 */
    ownerName: string
    welfareScene: string
    paymentTerms: string
    fulfillmentDeadline: string
    /** 卡券销售生效时形成应收所使用的业务到期日。 */
    receivableDueDate: string
    taxRatePercent: string
    remark: string
    lineItems: SalesOrderDraftLineInput[]
    intent: SalesOrderCreateIntent
    idempotencyKey: string
}

export type CreateSalesOrderResult = {
    salesOrderId: string
    documentNumber: string
    statusLabel: string
    createdAt: string
    reference: string
    /** 工作副本乐观锁版本；草稿意图时用于后续 `saveSalesOrderDraft` 续接编辑。 */
    workingCopyVersion?: number
    /** 创建后服务端返回的只读审批绑定；实物与卡券各自对应独立 DocumentType。 */
    approval?: DocumentApprovalView
}

export type ProgressTrack = {
    label: string
    tone: StatusTone
}

export type SalesOrderLineItem = {
    id: string
    name: string
    sku?: string
    /** 数量，十进制字符串 */
    quantity: string
    unit: string
    /** 含税单价 */
    unitPriceGross: string
    /** 含税小计 */
    amountGross: string
    /** 卡券：面额 */
    faceValue?: string
    /** 卡券：配赠率展示，如 5.00 */
    giftRate?: string
    /** 卡券：电子卡 / 实体卡 */
    cardForm?: string
    /** 采购责任解析使用的服务区域。 */
    serviceRegion?: string
    /** 对客户承诺的明细最晚交付日（实物/服务）。 */
    dueDate?: string
}

export type ProcurementProgressStatus = "pending" | "partial" | "covered"

export type SalesOrderProcurementProgress = {
    salesQuantity: string
    coveredQuantity: string
    remainingQuantity: string
    status: ProcurementProgressStatus
    label: string
    tone: StatusTone
}

type SalesOrderRelatedSummary = {
    purchaseOrders: number
    procurementProgress: SalesOrderProcurementProgress
    purchaseCreationAccess?: {
        allowed: boolean
        taskCount: number
        blocker?: string
    }
    fulfillments: number
    receipts: number
    invoices: number
}

/** 关闭资格：仅履约完成 + 应收结清；开票不阻塞。 */
type CloseEligibility = {
    fulfillmentComplete: boolean
    receivableSettled: boolean
    invoiceComplete: boolean
    /** 两项主条件均满足时由服务端自动关闭 */
    eligibleToClose: boolean
    blockers: string[]
    note: string
}

export type ActionBlocker = {
    action: string
    reason: string
}

export type SalesOrderRevisionLineSnapshot = {
    lineNo: number
    name: string
    spec?: string
    unit?: string
    amountGross: string
}

export type SalesOrderRevisionSnapshot = {
    revisionNo: number
    effectiveAt: string
    /** 合同精确修订标签，如 HT-2026-0312@v3；无合同时为空。 */
    contractRevisionLabel: string
    /** 客户主数据快照（不被当前值覆盖） */
    customerSnapshot: string
    amountGross: string
    amountNet: string
    taxAmount: string
    lineSummary: string
    settlementParty: string
    paymentTerm: string
    invoiceType: string
    taxPoint: string
    projectName: string
    businessRemark: string
    previousRevisionNo?: number
    changeOrderId?: string
    note: string
    lines: readonly SalesOrderRevisionLineSnapshot[]
}

export type SalesChangeOrderSummary = {
    id: string
    statusLabel: string
    statusTone: StatusTone
    /** 服务端状态码；审批相位只读此值与审批投影，不按影响路径推导。 */
    statusCode?: string
    /** 乐观锁版本；提交审批必须携带。 */
    version?: number
    baseRevisionNo: number
    createdAt: string
    impactPath: "procurement" | "operations"
    /** 统一只读审批结构。缺省表示列表行尚未补详情。 */
    approval?: DocumentApprovalView
}

export type FormalAllowedAction =
    | "START_SALES_CHANGE"
    | "REGISTER_ACCEPTANCE"
    | "VIEW_CLOSE_CONDITIONS"
    | "PRINT"
    | "EXPORT"

export type SalesOrderListItem = {
    id: string
    documentNumber: string
    customerName: string
    /** 合同稳定身份；无合同时为空。 */
    contractId: string
    contractNumber: string
    /** 合同上的公司名称（修订快照中的客户法定名称）。 */
    contractCompanyName: string
    /** 合同精确修订（快照），创建后随版本固定 */
    contractRevisionLabel: string
    nature: SalesOrderNature
    /** 创建来源：商城（MALL）或本系统（ERP），创建后恒不变 */
    originSystem: SalesOrderOrigin
    /**
     * `code` 是服务端权威阶段码，与 `lib/filter-orders.ts::SALES_ORDER_STATUS_OPTIONS` 对齐。
     * `ownerRole`/`ownerUserId`/`ownerUserName`/`dueAt` 是当前阶段命中的待办责任人与
     * 时限（审核轨在途时才有值，服务端整页批量解析，列表与详情共用同一来源）。
     */
    primaryStatus: {
        code: string
        label: string
        tone: StatusTone
        ownerRole?: string | null
        ownerUserId?: string | null
        ownerUserName?: string | null
        /** 预计完成时限（秒级时间戳）。 */
        dueAt?: number | null
    }
    fulfillment: ProgressTrack
    collection: ProgressTrack
    invoicing: ProgressTrack
    /** 含税成交金额 */
    amountGross: string
    /** 不含税金额 */
    amountNet: string
    /** 税额 */
    taxAmount: string
    /** 已回款（含税口径展示） */
    receivedAmount: string
    /** 已开票 */
    invoicedAmount: string
    /** 负责销售用户 id；撤回未审结审批等「本人」判定用此字段。 */
    ownerUserId: string
    ownerName: string
    submittedAt: string
    welfareScene: string
    remark?: string
    version: number
    lockVersion: number
    /** 当前生效版本号；尚未形成正式版本时为 `null`，不得用实体乐观锁 `version` 代替。 */
    currentRevisionNo: number | null
    settlementEntity: string
    sellerEntity: string
    paymentTerms: string
    /** 建单时统一填写的销项税率百分数，如 13.00。 */
    taxRatePercent?: string
    /** 表头履约期限（卡券全单；实物为客户承诺期限摘要） */
    fulfillmentDeadline: string
    /** 卡券最终通过后形成应收所使用的到期日。 */
    receivableDueDate?: string
    customerContact?: string
    lineItems: readonly SalesOrderLineItem[]
    related: SalesOrderRelatedSummary
    closeEligibility: CloseEligibility
    /** 业务性质创建后不可修改 */
    natureLocked: true
    /** 商城开单（origin=mall）同步期间商业字段只读 */
    commercialReadOnly: boolean
    commercialReadOnlyReason?: string
    revisions: readonly SalesOrderRevisionSnapshot[]
    /** 统一只读审批结构；实物为 SalesOrder，卡券为 VoucherSalesOrder。 */
    approval?: DocumentApprovalView
    activeChangeOrder?: SalesChangeOrderSummary | null
    allowedActions: FormalAllowedAction[]
    actionBlockers: ActionBlocker[]
}
