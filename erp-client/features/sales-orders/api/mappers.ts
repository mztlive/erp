import type { StatusTone } from "@/components/ui/status-badge"
import type {
    BackendCloseEligibility,
    BackendProcurementConfirmation,
    BackendRevision,
    BackendSalesChangeOrder,
    BackendSalesOrderDetail,
    BackendSalesOrderReview,
    BackendSalesOrderView,
    BackendWorkingCopyLine,
    SalesOrdersListQuery,
} from "@/features/sales-orders/api/contracts"
import type {
    ActionBlocker,
    CardSalesApproval,
    FormalAllowedAction,
    ProcurementRejectionResolution,
    ProgressTrack,
    SalesChangeOrderSummary,
    SalesOrderLineItem,
    SalesOrderListItem,
    SalesOrderNature,
    SalesOrderOrigin,
    SalesOrderRevisionSnapshot,
} from "@/features/sales-orders/types"
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

function toneForStatus(label: string): StatusTone {
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
            return { label: "已完成", tone: "success" }
        default:
            return { label: "未开", tone: "neutral" }
    }
}

/**
 * 结案条件卡只在详情页展示（`close-conditions-card.tsx`），列表行从不渲染
 * 这个字段；纯列表拉取（`sales_order_list`）不携带后端权威结案资格，避免为
 * 不可见字段逐行加查询成本。详情路径（`mapDetailToListItem`）会用
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

function mapWorkingCopyLines(
    lines: BackendWorkingCopyLine[] | undefined,
): SalesOrderLineItem[] {
    if (!lines?.length) return []
    return lines.map((line) => {
        const isVoucher = line.line_type === "VOUCHER"
        const item: SalesOrderLineItem = {
            id: line.sales_order_line_id || line.id,
            name: line.item_name_snapshot,
            sku: line.spec_snapshot || line.sku_id || undefined,
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
            item.cardForm =
                line.card_form === "PHYSICAL"
                    ? "实体卡"
                    : line.card_form === "ELECTRONIC"
                      ? "电子卡"
                      : (line.card_form ?? undefined)
        }
        return item
    })
}

function mapRevisions(
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
        note: rev.revision_source,
    }))
}

function defaultAllowedActions(
    commercial: string,
    hasCardApproval: boolean,
    hasRejection: boolean,
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

    if (commercial === "EFFECTIVE" || commercial === "PENDING_REVIEW") {
        allowed.push("REGISTER_ACCEPTANCE")
    }

    if (hasRejection) {
        allowed.push("RESOLVE_PROCUREMENT_REJECTION")
    }
    if (hasCardApproval) {
        allowed.push("HANDLE_CARD_APPROVAL")
    }

    return { allowed, blockers }
}

export function mapListItemFromBackend(
    row: BackendSalesOrderView,
    extras?: {
        customerName?: string
        contractNumber?: string
        amountGross?: string
        amountNet?: string
        taxAmount?: string
        lineItems?: SalesOrderLineItem[]
        ownerName?: string
        paymentTerms?: string
        welfareScene?: string
        fulfillmentDeadline?: string
        remark?: string
        settlementEntity?: string
        revisions?: SalesOrderRevisionSnapshot[]
        procurementRejection?: ProcurementRejectionResolution | null
        activeCardSalesApproval?: CardSalesApproval | null
        activeChangeOrder?: SalesChangeOrderSummary | null
        customerContact?: string
        closeEligibility?: BackendCloseEligibility
        startSalesChange?: { allowed: boolean; blocker?: string | null }
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
    const hasCard = Boolean(extras?.activeCardSalesApproval)
    const hasRejection = Boolean(extras?.procurementRejection)
    const { allowed, blockers } = defaultAllowedActions(
        row.commercial_status,
        hasCard,
        hasRejection,
        extras?.startSalesChange,
    )

    const commercialReadOnly =
        originSystem === "mall" ||
        primaryStatus.label === "已关闭" ||
        primaryStatus.label === "已作废" ||
        hasCard ||
        row.commercial_status === "EFFECTIVE"

    return {
        id: row.id,
        documentNumber: row.order_no,
        customerName: extras?.customerName ?? row.customer_id,
        contractNumber: extras?.contractNumber ?? row.contract_id ?? "",
        contractRevisionLabel: extras?.contractNumber
            ? `${extras.contractNumber}`
            : "",
        nature,
        originSystem,
        primaryStatus,
        fulfillment,
        collection,
        invoicing,
        amountGross: extras?.amountGross ?? "0.00",
        amountNet: extras?.amountNet ?? "0.00",
        taxAmount: extras?.taxAmount ?? "0.00",
        receivedAmount: "0.00",
        invoicedAmount: "0.00",
        ownerName: extras?.ownerName ?? "",
        submittedAt: formatInstant(row.created_at),
        welfareScene: extras?.welfareScene ?? "",
        remark: extras?.remark,
        version: Number(row.version) || 1,
        lockVersion: Number(row.version) || 1,
        settlementEntity: extras?.settlementEntity ?? "",
        sellerEntity: "",
        paymentTerms: extras?.paymentTerms ?? "",
        fulfillmentDeadline: extras?.fulfillmentDeadline ?? "",
        customerContact: extras?.customerContact,
        lineItems: extras?.lineItems ?? [],
        related: {
            purchaseOrders: 0,
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
        procurementRejection: extras?.procurementRejection ?? null,
        activeCardSalesApproval: extras?.activeCardSalesApproval ?? null,
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

/**
 * 详情商业内容优先：可编辑草稿 → 最新提交快照。
 * 提交后 working_copy 为空时必须从 submission 回填明细与表头。
 */
function pickCommercialContent(detail: BackendSalesOrderDetail): {
    lines: BackendWorkingCopyLine[]
    amountGross?: string
    amountNet?: string
    taxAmount?: string
    ownerUserId: string
    customerName?: string
    contractNo?: string
    settlementPartyName?: string
    paymentTerms: string
    welfareScene: string
    fulfillmentDeadline: string
    remark?: string
} {
    const wc = detail.working_copy
    const submissions = [...(detail.submissions ?? [])].sort(
        (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0),
    )
    const latestSubmission = submissions[0]

    if (wc?.lines?.length) {
        return {
            lines: wc.lines,
            amountGross: wc.gross_amount,
            amountNet: wc.net_amount,
            taxAmount: wc.tax_amount,
            ownerUserId:
                wc.editor_user_id || latestSubmission?.submitted_by || "",
            customerName: wc.customer_name || undefined,
            contractNo: wc.contract_no || undefined,
            settlementPartyName: wc.settlement_party_name || undefined,
            paymentTerms: wc.payment_term_name || wc.payment_term_code || "",
            welfareScene: wc.project_name || "",
            fulfillmentDeadline: formatEpochDate(wc.voucher_expiry_at),
            remark: wc.business_remark || undefined,
        }
    }

    if (latestSubmission) {
        return {
            lines: latestSubmission.lines ?? [],
            amountGross: latestSubmission.gross_amount,
            amountNet: latestSubmission.net_amount,
            taxAmount: latestSubmission.tax_amount,
            ownerUserId: latestSubmission.submitted_by || "",
            customerName: latestSubmission.customer_name || undefined,
            contractNo: latestSubmission.contract_no || undefined,
            settlementPartyName:
                latestSubmission.settlement_party_name || undefined,
            paymentTerms:
                latestSubmission.payment_term_name ||
                latestSubmission.payment_term_code ||
                "",
            welfareScene: latestSubmission.project_name || "",
            fulfillmentDeadline: formatEpochDate(
                latestSubmission.voucher_expiry_at,
            ),
            remark: latestSubmission.business_remark || undefined,
        }
    }

    return {
        lines: [],
        ownerUserId: "",
        paymentTerms: "",
        welfareScene: "",
        fulfillmentDeadline: "",
    }
}

export function mapDetailToListItem(
    detail: BackendSalesOrderDetail,
    extras?: {
        customerName?: string
        contractNumber?: string
        ownerName?: string
        procurementRejection?: ProcurementRejectionResolution | null
        activeCardSalesApproval?: CardSalesApproval | null
        activeChangeOrder?: SalesChangeOrderSummary | null
        customerContact?: string
    },
): SalesOrderListItem {
    const commercial = pickCommercialContent(detail)
    return mapListItemFromBackend(
        {
            id: detail.id,
            order_no: detail.order_no,
            business_type: detail.business_type,
            origin_system: detail.origin_system,
            customer_id: detail.customer_id,
            contract_id: detail.contract_id,
            commercial_status: detail.commercial_status,
            review_status: detail.review_status,
            fulfillment_progress: detail.fulfillment_progress,
            collection_progress: detail.collection_progress,
            invoice_progress: detail.invoice_progress,
            close_status: detail.close_status,
            effective_at: detail.effective_at,
            version: detail.version,
            created_at: detail.created_at,
            updated_at: detail.created_at,
            stage: detail.stage,
        },
        {
            customerName:
                extras?.customerName ||
                commercial.customerName ||
                detail.customer_id,
            contractNumber:
                extras?.contractNumber || commercial.contractNo || "",
            amountGross: commercial.amountGross,
            amountNet: commercial.amountNet,
            taxAmount: commercial.taxAmount,
            lineItems: mapWorkingCopyLines(commercial.lines),
            ownerName: extras?.ownerName || commercial.ownerUserId || "",
            customerContact: extras?.customerContact,
            paymentTerms: commercial.paymentTerms,
            welfareScene: commercial.welfareScene,
            fulfillmentDeadline: commercial.fulfillmentDeadline,
            remark: commercial.remark,
            revisions: mapRevisions(detail.revisions),
            procurementRejection: extras?.procurementRejection,
            activeCardSalesApproval: extras?.activeCardSalesApproval,
            activeChangeOrder: extras?.activeChangeOrder,
            settlementEntity:
                commercial.settlementPartyName || detail.settlement_party_id,
            closeEligibility: detail.close_eligibility,
            startSalesChange: {
                allowed: detail.can_start_sales_change_order,
                blocker: detail.change_order_blocker,
            },
        },
    )
}

export function mapReviewToCardApproval(
    review: BackendSalesOrderReview,
): CardSalesApproval | null {
    if (review.status !== "PENDING") return null
    const isLeader = review.review_stage === "SALES_LEADER"
    const isOps = review.review_stage === "OPERATIONS"
    if (!isLeader && !isOps) return null
    return {
        workItemId: review.id,
        workItemType: isLeader
            ? "CARD_SALES_MANAGER_APPROVAL"
            : "CARD_SALES_OPERATION_APPROVAL",
        workItemStatus: review.reviewer_id ? "CLAIMED" : "UNCLAIMED",
        subjectVersion: review.submission_id,
        subjectHash: review.submission_id,
        claimedByLabel: review.reviewer_id ?? undefined,
        frozenSubmissionSummary: "",
        expectedReviewStatus: isLeader
            ? "PENDING_SALES_LEAD"
            : "PENDING_OPERATIONS",
        allowedActions: review.reviewer_id ? ["APPROVE", "REJECT"] : ["CLAIM"],
        actionBlockers: review.reviewer_id
            ? []
            : [
                  { action: "APPROVE", reason: "请先领取后再审批。" },
                  { action: "REJECT", reason: "请先领取后再审批。" },
              ],
    }
}

export function mapRejectedProcurement(
    conf: BackendProcurementConfirmation,
): ProcurementRejectionResolution | null {
    if (conf.status !== "REJECTED") return null
    return {
        rejectedProcurementConfirmationId: conf.id,
        rejectedProcurementWorkItemId: "",
        rejectedSubmissionId: conf.submission_id,
        rejectedSubmissionNo: 0,
        rejectedSubjectHash: conf.submission_id,
        rejectReasonCode: "",
        rejectComment: "",
        rejectedByLabel: conf.handled_by ?? "",
        rejectedAt: formatInstant(conf.handled_at),
        reviewStatus: "REJECTED",
        draftDifference: {
            changedItemOrService: false,
            changedSalesPrice: false,
            commercialTermsUnchanged: true,
            diffSummary: [],
        },
        fixedResolutions: ["RESUBMIT_CHANGED_TERMS", "VOID_AFTER_REJECTION"],
        allowedActions: ["RESUBMIT_CHANGED_TERMS", "VOID_AFTER_REJECTION"],
        actionBlockers: [],
    }
}

export function mapChangeOrder(
    row: BackendSalesChangeOrder,
    nature: SalesOrderNature,
): SalesChangeOrderSummary {
    const impactPath = nature === "card_voucher" ? "operations" : "procurement"
    const statusLabel =
        row.status === "PENDING_IMPACT_CONFIRMATION"
            ? impactPath === "operations"
                ? "待运营执行影响确认"
                : "待采购履约影响确认"
            : row.status === "PENDING_FINANCE_REVIEW"
              ? "待财务复核"
              : row.status === "DRAFT"
                ? "草稿"
                : row.status === "EFFECTIVE"
                  ? "已生效"
                  : row.status === "VOIDED"
                    ? "已作废"
                    : row.status
    return {
        id: row.id,
        statusLabel,
        statusTone: toneForStatus(statusLabel),
        baseRevisionNo: 0,
        createdAt: new Date(row.created_at * 1000).toISOString(),
        impactPath,
    }
}

export function mapStatusFilterToBackend(status?: string): {
    commercial_status?: string
    review_status?: string
} {
    switch (status) {
        case "draft":
            return { commercial_status: "DRAFT" }
        case "voided":
            return { commercial_status: "VOIDED" }
        case "effective":
            return { commercial_status: "EFFECTIVE" }
        case "closed":
            return { commercial_status: "EFFECTIVE" }
        case "awaiting_confirm":
            return {
                commercial_status: "PENDING_REVIEW",
                review_status: "PENDING_PROCUREMENT_CONFIRMATION",
            }
        case "awaiting_sales":
            return {
                commercial_status: "PENDING_REVIEW",
                review_status: "REJECTED",
            }
        case "awaiting_sales_lead":
            return {
                commercial_status: "PENDING_REVIEW",
                review_status: "PENDING_SALES_LEADER",
            }
        case "awaiting_ops":
            return {
                commercial_status: "PENDING_REVIEW",
                review_status: "PENDING_OPERATIONS",
            }
        case "fulfilling":
            return { commercial_status: "EFFECTIVE" }
        default:
            return {}
    }
}

export function mapSortBy(
    sortBy?: SalesOrdersListQuery["sortBy"],
): string | undefined {
    if (!sortBy) return undefined
    if (sortBy === "documentNumber") return "order_no"
    if (sortBy === "submittedAt") return "created_at"
    // amountGross / contractNumber / ownerName 不在后端白名单
    return "created_at"
}

export function mapFulfillmentMode(label: string): string {
    const t = label.trim()
    if (t.includes("直发")) return "SUPPLIER_DIRECT"
    if (t.includes("电子")) return "ELECTRONIC_DELIVERY"
    if (t.includes("服务") || t.includes("线下")) return "OFFLINE_SERVICE"
    return "COMPANY_WAREHOUSE"
}

export function mapCardForm(label: string): string {
    return label.includes("实体") ? "PHYSICAL" : "ELECTRONIC"
}

/** 福利场景：表单码 / 中文 → 后端 SCREAMING_SNAKE_CASE；无法识别则 null。 */
export function mapWelfareScenarioCode(raw: string): string | null {
    const value = raw.trim()
    if (!value) return null
    switch (value) {
        case "ANNUAL_GIFT_BAG":
        case "年节礼包":
            return "ANNUAL_GIFT_BAG"
        case "MEAL_SUBSIDY":
        case "餐补":
            return "MEAL_SUBSIDY"
        case "CONDOLENCE_GIFT":
        case "慰问品":
            return "CONDOLENCE_GIFT"
        case "CONSUMPTION_FUND":
        case "消费金":
            return "CONSUMPTION_FUND"
        case "OTHER":
        case "其他":
        case "其它":
            return "OTHER"
        default:
            return null
    }
}

export function percentToRate(percent: string): string {
    const n = Number(percent)
    if (!Number.isFinite(n)) return "0.000000"
    return (n / 100).toFixed(6)
}

export function rateToPercent(rate: string | undefined): string {
    const n = Number(rate)
    if (!Number.isFinite(n)) return "13.00"
    return (n * 100).toFixed(2)
}

export function mapCardFormFromBackend(
    code: string | null | undefined,
): string {
    return code === "PHYSICAL" ? "实体卡" : "电子卡"
}

export function dateToUnixSecs(dateStr: string): number {
    if (!dateStr) return Math.floor(Date.now() / 1000)
    // YYYY-MM-DD or datetime
    const normalized =
        dateStr.length === 10 ? `${dateStr}T00:00:00+08:00` : dateStr
    const ms = Date.parse(normalized)
    if (Number.isNaN(ms)) return Math.floor(Date.now() / 1000)
    return Math.floor(ms / 1000)
}

export function localOrderNo(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`
    return `XS${stamp}`
}
