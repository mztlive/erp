import type { StatusTone } from "@/components/ui/status-badge"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    BackendCloseEligibility,
    BackendPurchaseCoverage,
    BackendRevision,
    BackendSalesOrderView,
    BackendWorkingCopyLine,
} from "@/features/sales-orders/api/contracts"
import type {
    ActionBlocker,
    FormalAllowedAction,
    ProgressTrack,
    SalesOrderProcurementProgress,
    SalesChangeOrderSummary,
    SalesOrderLineItem,
    SalesOrderListItem,
    SalesOrderNature,
    SalesOrderOrigin,
    SalesOrderRevisionSnapshot,
} from "@/features/sales-orders/types"
import { deriveVoucherGiftPreview } from "@/features/sales-orders/lib/sales-order-create-model"
import type { ApiError } from "@/lib/api/errors"

const validationError = (message: string): ApiError => ({
    kind: "Validation",
    message,
    status: 400,
})

export function throwValidation(message: string): never {
    throw validationError(message)
}

export function formatInstant(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    const d = new Date(secs * 1000)
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function formatIsoNow(): string {
    return new Date().toISOString()
}

export function mapNature(businessType: string): SalesOrderNature {
    return businessType === "VOUCHER" ? "card_voucher" : "physical_service"
}

function mapOrigin(origin: string): SalesOrderOrigin {
    return origin === "MALL" ? "mall" : "erp"
}

export function toneForStatus(label: string): StatusTone {
    if (label.includes("作废") || label.includes("关闭")) return "void"
    if (
        label.includes("生效") ||
        label.includes("完成") ||
        label.includes("通过")
    )
        return "success"
    if (
        label.includes("待") ||
        label.includes("审核") ||
        label.includes("确认") ||
        label.includes("审批")
    )
        return "warning"
    if (label.includes("履约") || label.includes("部分")) return "info"
    return "neutral"
}

/** 后端 `stage.tone` 映射为前端徽标语气；后端只产出 `neutral` 兜底值以外的语气。 */
function mapStageTone(tone: string): StatusTone {
    return tone as StatusTone
}

function mapFulfillment(code: string): ProgressTrack {
    switch (code) {
        case "PARTIALLY_FULFILLED":
            return { label: "部分履约", tone: "warning" }
        case "COMPLETED":
            return { label: "已完成", tone: "success" }
        default:
            return { label: "未开始", tone: "neutral" }
    }
}

function mapCollection(code: string): ProgressTrack {
    switch (code) {
        case "PARTIALLY_COLLECTED":
            return { label: "部分回款", tone: "warning" }
        case "SETTLED":
            return { label: "已结清", tone: "success" }
        default:
            return { label: "未收", tone: "neutral" }
    }
}

function mapInvoicing(code: string): ProgressTrack {
    switch (code) {
        case "PARTIALLY_INVOICED":
            return { label: "部分开票", tone: "warning" }
        case "COMPLETED":
            return { label: "已开齐", tone: "success" }
        default:
            return { label: "未开", tone: "neutral" }
    }
}

export function mapProcurementProgress(
    coverage?: BackendPurchaseCoverage | null,
): SalesOrderProcurementProgress {
    const salesQuantity = coverage?.total_quantity ?? "0"
    const coveredQuantity = coverage?.covered_quantity ?? "0"
    const remainingQuantity = coverage?.remaining_quantity ?? "0"
    const progress = Number(coverage?.progress ?? "0")
    const status =
        Number(salesQuantity) > 0 &&
        (Number(remainingQuantity) <= 0 || progress >= 1)
            ? "covered"
            : Number(coveredQuantity) > 0 || progress > 0
              ? "partial"
              : "pending"

    if (status === "covered") {
        return {
            salesQuantity,
            coveredQuantity,
            remainingQuantity,
            status,
            label: "采购已覆盖",
            tone: "success",
        }
    }
    if (status === "partial") {
        return {
            salesQuantity,
            coveredQuantity,
            remainingQuantity,
            status,
            label: "部分采购",
            tone: "warning",
        }
    }
    return {
        salesQuantity,
        coveredQuantity,
        remainingQuantity,
        status,
        label: "待采购",
        tone: "neutral",
    }
}

/**
 * 结案资格只在详情路径使用（生命周期轨、履约焦点等）；列表行从不渲染。
 * 纯列表拉取（`sales_order_list`）不携带后端权威结案资格，避免为不可见字段
 * 逐行加查询成本。详情路径（`mapDetailToListItem`）会用
 * `detail.close_eligibility` 覆盖这个占位值。
 */
const LIST_ROW_CLOSE_ELIGIBILITY_PLACEHOLDER: SalesOrderListItem["closeEligibility"] =
    {
        fulfillmentComplete: false,
        receivableSettled: false,
        invoiceComplete: false,
        eligibleToClose: false,
        blockers: [],
        note: "",
    }

function mapCloseEligibilityFromBackend(
    backend: BackendCloseEligibility,
): SalesOrderListItem["closeEligibility"] {
    return {
        fulfillmentComplete: backend.fulfillment_complete,
        receivableSettled: backend.receivable_settled,
        invoiceComplete: backend.invoice_complete,
        eligibleToClose: backend.eligible_to_close,
        blockers: backend.blockers,
        note: backend.note,
    }
}

/** 后端履约方式码 → 建单/详情展示中文；无法识别时原样回退。 */
export function mapFulfillmentModeFromBackend(
    code: string | null | undefined,
): string {
    switch ((code ?? "").trim()) {
        case "COMPANY_WAREHOUSE":
            return "公司仓发"
        case "SUPPLIER_DIRECT":
            return "供应商直发"
        case "ELECTRONIC_DELIVERY":
            return "电子交付"
        case "OFFLINE_SERVICE":
            return "线下服务"
        default:
            return code?.trim() || ""
    }
}

export function mapWorkingCopyLines(
    lines: BackendWorkingCopyLine[] | undefined,
): SalesOrderLineItem[] {
    if (!lines?.length) return []
    return lines.map((line) => {
        const isVoucher = line.line_type === "VOUCHER"
        const specSnapshot = line.spec_snapshot?.trim()
        const skuId = line.sku_id?.trim()
        const item: SalesOrderLineItem = {
            id: line.sales_order_line_id || line.id,
            name: line.item_name_snapshot,
            // 当前建单会把稳定 SKU id 同时写进 spec_snapshot；详情不得回显内部 id。
            sku:
                specSnapshot && specSnapshot !== skuId
                    ? specSnapshot
                    : undefined,
            quantity: isVoucher
                ? String(line.card_count ?? line.quantity ?? "0")
                : (line.quantity ?? "0"),
            unit:
                line.unit_snapshot ||
                line.base_unit_code ||
                (isVoucher ? "张" : ""),
            unitPriceGross: line.unit_price_gross ?? "0.00",
            amountGross: line.gross_amount ?? line.transaction_amount ?? "0.00",
        }
        if (isVoucher) {
            item.faceValue = line.face_value ?? undefined
            const gift = deriveVoucherGiftPreview(
                line.face_value ?? "",
                line.unit_price_gross ?? "",
                String(line.card_count ?? line.quantity ?? ""),
            )
            item.giftRate = gift?.giftRatePercent
            item.cardForm =
                line.card_form === "PHYSICAL"
                    ? "实体卡"
                    : line.card_form === "ELECTRONIC"
                      ? "电子卡"
                      : (line.card_form ?? undefined)
        } else {
            const fulfillmentMode = mapFulfillmentModeFromBackend(
                line.fulfillment_mode,
            )
            if (fulfillmentMode) item.fulfillmentMode = fulfillmentMode
            const serviceRegion = line.service_region?.trim()
            if (serviceRegion) item.serviceRegion = serviceRegion
            const dueDate = formatEpochDate(line.fulfillment_due_at)
            if (dueDate) item.dueDate = dueDate
        }
        return item
    })
}

const REVISION_SOURCE_LABEL: Record<string, string> = {
    ERP_APPROVAL: "审批生效",
    SALES_CHANGE: "销售变更单",
    MALL_SYNC: "商城同步",
}

export function mapRevisions(
    revisions: BackendRevision[] | undefined,
): SalesOrderRevisionSnapshot[] {
    if (!revisions?.length) return []
    return revisions.map((rev) => ({
        revisionNo: rev.revision_no,
        effectiveAt: formatInstant(rev.effective_at),
        contractRevisionLabel: "",
        customerSnapshot: "",
        amountGross: rev.gross_amount,
        lineSummary: "",
        note: REVISION_SOURCE_LABEL[rev.revision_source] ?? rev.revision_source,
    }))
}

function defaultAllowedActions(
    commercial: string,
    fulfillment: string,
    close: string,
    /**
     * 能否发起销售变更单——后端权威判定（`sales_order_detail` 的
     * `can_start_sales_change_order`/`change_order_blocker`）。纯列表拉取没有
     * 这份数据（避免为不可见的行内操作逐行查询），此时不下结论：既不放进
     * `allowed` 也不放进 `blockers`，不在前端重新猜测规则。
     */
    startSalesChange?: { allowed: boolean; blocker?: string | null },
): { allowed: FormalAllowedAction[]; blockers: ActionBlocker[] } {
    const allowed: FormalAllowedAction[] = [
        "PRINT",
        "EXPORT",
        "VIEW_CLOSE_CONDITIONS",
    ]
    const blockers: ActionBlocker[] = []

    if (startSalesChange?.allowed) {
        allowed.push("START_SALES_CHANGE")
    } else if (startSalesChange && startSalesChange.blocker) {
        blockers.push({
            action: "START_SALES_CHANGE",
            reason: startSalesChange.blocker,
        })
    }

    // 客户验收属于已生效后的履约收口；审批中不得开放。
    // 此处只表示生命周期允许登记；详情页还要核对尚未验收完的履约事实，
    // 发货/交付尚未发生时不得当成当前待办。
    if (
        commercial === "EFFECTIVE" &&
        fulfillment !== "COMPLETED" &&
        close !== "CLOSED"
    ) {
        allowed.push("REGISTER_ACCEPTANCE")
    }

    return { allowed, blockers }
}

export function mapListItemFromBackend(
    row: BackendSalesOrderView,
    extras?: {
        customerName?: string
        contractNumber?: string
        contractRevisionLabel?: string
        contractCompanyName?: string
        amountGross?: string
        amountNet?: string
        taxAmount?: string
        receivedAmount?: string
        invoicedAmount?: string
        lineItems?: SalesOrderLineItem[]
        ownerUserId?: string
        ownerName?: string
        paymentTerms?: string
        taxRatePercent?: string
        welfareScene?: string
        fulfillmentDeadline?: string
        targetMallName?: string
        receivableDueDate?: string
        remark?: string
        settlementEntity?: string
        revisions?: SalesOrderRevisionSnapshot[]
        approval?: DocumentApprovalView
        activeChangeOrder?: SalesChangeOrderSummary | null
        customerContact?: string
        closeEligibility?: BackendCloseEligibility
        startSalesChange?: { allowed: boolean; blocker?: string | null }
        currentRevisionNo?: number | null
        purchaseOrderCount?: number
        purchaseCreationAccess?: {
            allowed: boolean
            taskCount: number
            blocker?: string
        }
    },
): SalesOrderListItem {
    const nature = mapNature(row.business_type)
    const originSystem = mapOrigin(row.origin_system)
    const primaryStatus = {
        code: row.stage.code,
        label: row.stage.label,
        tone: mapStageTone(row.stage.tone),
        ownerRole: row.stage.owner_role,
        ownerUserId: row.stage.owner_user_id,
        ownerUserName: row.stage.owner_user_name,
        dueAt: row.stage.due_at,
    }
    const fulfillment = mapFulfillment(row.fulfillment_progress)
    const collection = mapCollection(row.collection_progress)
    const invoicing = mapInvoicing(row.invoice_progress)
    const hasRunningVoucherApproval =
        nature === "card_voucher" &&
        (Boolean(extras?.approval?.instance) ||
            row.commercial_status === "PENDING_REVIEW" ||
            row.review_status === "IN_APPROVAL")
    const { allowed, blockers } = defaultAllowedActions(
        row.commercial_status,
        row.fulfillment_progress,
        row.close_status,
        extras?.startSalesChange,
    )

    const commercialReadOnly =
        originSystem === "mall" ||
        primaryStatus.label === "已关闭" ||
        primaryStatus.label === "已作废" ||
        hasRunningVoucherApproval ||
        row.commercial_status === "EFFECTIVE"

    return {
        id: row.id,
        documentNumber: row.order_no,
        customerName: extras?.customerName ?? row.customer_id,
        contractId: row.contract_id ?? "",
        contractNumber: extras?.contractNumber ?? "",
        contractCompanyName:
            extras?.contractCompanyName ?? extras?.customerName ?? "",
        contractRevisionLabel:
            extras?.contractRevisionLabel ?? extras?.contractNumber ?? "",
        nature,
        originSystem,
        primaryStatus,
        fulfillment,
        collection,
        invoicing,
        amountGross: extras?.amountGross ?? "0.00",
        amountNet: extras?.amountNet ?? "0.00",
        taxAmount: extras?.taxAmount ?? "0.00",
        receivedAmount: extras?.receivedAmount ?? "0.00",
        invoicedAmount: extras?.invoicedAmount ?? "0.00",
        ownerUserId: extras?.ownerUserId ?? row.owner_user_id ?? "",
        ownerName: extras?.ownerName ?? row.owner_user_name?.trim() ?? "",
        submittedAt: formatInstant(row.created_at),
        welfareScene: extras?.welfareScene ?? "",
        remark: extras?.remark,
        version: Number(row.version) || 1,
        lockVersion: Number(row.version) || 1,
        currentRevisionNo: extras?.currentRevisionNo ?? null,
        settlementEntity: extras?.settlementEntity ?? "",
        sellerEntity: "",
        paymentTerms: extras?.paymentTerms ?? "",
        taxRatePercent: extras?.taxRatePercent ?? "",
        fulfillmentDeadline: extras?.fulfillmentDeadline ?? "",
        targetMallName: extras?.targetMallName,
        receivableDueDate: extras?.receivableDueDate,
        customerContact: extras?.customerContact,
        lineItems: extras?.lineItems ?? [],
        related: {
            purchaseOrders: extras?.purchaseOrderCount ?? 0,
            procurementProgress: mapProcurementProgress(row.purchase_coverage),
            purchaseCreationAccess: extras?.purchaseCreationAccess,
            fulfillments: 0,
            receipts: 0,
            invoices: 0,
        },
        closeEligibility: extras?.closeEligibility
            ? mapCloseEligibilityFromBackend(extras.closeEligibility)
            : LIST_ROW_CLOSE_ELIGIBILITY_PLACEHOLDER,
        natureLocked: true,
        commercialReadOnly,
        commercialReadOnlyReason: commercialReadOnly
            ? originSystem === "mall"
                ? "这单由商城开单，商业数据同步中，本系统只读；改内容请在商城处理。"
                : row.commercial_status === "EFFECTIVE"
                  ? "本单已生效，不能直接改；改内容请「发起改单」。"
                  : undefined
            : undefined,
        revisions: extras?.revisions ?? [],
        approval: extras?.approval,
        activeChangeOrder: extras?.activeChangeOrder ?? null,
        allowedActions: allowed,
        actionBlockers: blockers,
    }
}

export function formatEpochDate(secs?: number | null): string {
    if (secs == null || !Number.isFinite(secs) || secs <= 0) return ""
    try {
        const d = new Date(secs * 1000)
        if (Number.isNaN(d.getTime())) return ""
        const y = d.getFullYear()
        const m = String(d.getMonth() + 1).padStart(2, "0")
        const day = String(d.getDate()).padStart(2, "0")
        return `${y}-${m}-${day}`
    } catch {
        return ""
    }
}
