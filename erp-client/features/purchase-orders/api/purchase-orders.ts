/**
 * W08 采购单 · 真实 HTTP API。
 * 契约形状保持 features/purchase-orders/types.ts 与 queries.ts 不变；
 * 后端差异在本文件内适配，缺口登记见 docs/dev-plan/p4-evidence/F4.md。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { ApiError } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"
import type {
    CreatePurchaseOrderFromBasisInput,
    FormalActionResponse,
    FulfillmentResponsibility,
    PaymentGateState,
    PurchaseCreationBasis,
    PurchaseOrderCenterView,
    PurchaseOrderListItem,
    PurchaseOrderMetricFilter,
    PurchaseOrderStatus,
    PurchaseOrderStatusFilter,
    PurchaseReviewStatus,
    PurchaseType,
    ReviewPurchaseOrderInput,
    SavePurchaseOrderDraftInput,
    SubmitPurchaseOrderInput,
} from "@/features/purchase-orders/types"
import {
    PAYMENT_TERM_OPTIONS,
    PO_STATUS_LABEL,
    PO_STATUS_TONE,
    REVIEW_STATUS_LABEL,
} from "@/features/purchase-orders/types"
import type {
    WorkItemAllowedAction,
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items"

export type PurchaseOrderListQuery = {
    q?: string
    status?: PurchaseOrderStatusFilter
    metric?: PurchaseOrderMetricFilter
    page?: number
    pageSize?: number
    sortBy?: string
    sortDir?: "asc" | "desc"
}

export type PurchaseOrderListResult = {
    rows: PurchaseOrderListItem[]
    total: number
    page: number
    pageSize: number
    metrics: Array<{
        key: string
        label: string
        count: number
        detail: string
    }>
    freshness: { updatedAt: string; state: "fresh" }
}

const PURCHASE_ORDER_DEFAULT_PAGE_SIZE = 20
const PURCHASE_ORDER_MAX_PAGE_SIZE = 100

// ─── Backend wire types ──────────────────────────────────────────────────────

type BackendPage<T> = {
    items: T[]
    total: number
    page?: number
    page_size?: number
}

type BackendListItem = {
    id: string
    purchase_no: string
    sales_order_id: string
    supplier_id: string
    supplier_name: string
    purchase_type: PurchaseType | string
    status: string
    review_status: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    payment_progress: string
    invoice_progress: string
    fulfillment_progress: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    version: number
    created_at: number
}

type BackendLine = {
    line_id: string
    line_no: number
    line_type: "ITEM_SERVICE" | "LOGISTICS_FEE" | string
    procurement_confirmation_line_id?: string | null
    sku_id?: string | null
    sku_revision_id?: string | null
    product_name?: string | null
    specification?: string | null
    quantity?: string | null
    base_unit_code?: string | null
    unit_cost_gross?: string | null
    input_tax_rate?: string | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    expected_delivery_date?: string | null
    sales_order_submission_line_id?: string | null
    allocated_quantity?: string | null
}

type BackendCenter = {
    id: string
    purchase_no: string
    status: string
    review_status: string
    version: number
    sales_order_id: string
    supplier_id: string
    supplier_name: string
    purchase_type: PurchaseType | string
    payment_term_code: string
    fulfillment_responsibility: FulfillmentResponsibility | string
    payment_progress: string
    invoice_progress: string
    fulfillment_progress: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    revision_no?: number | null
    content_source: string
    lines: BackendLine[]
    totals: { gross: string; net: string; tax: string }
    allocations: Array<{
        id: string
        purchase_order_revision_line_id: string
        sales_order_revision_line_id: string
        allocated_quantity: string
        allocated_cost_gross: string
        allocated_cost_net: string
    }>
    changes: Array<{
        change_id: string
        status: string
        base_revision_id: string
        effective_revision_id?: string | null
        reason: string
        created_at: number
    }>
    review_work_item?: {
        work_item_id: string
        work_item_type: "PURCHASE_ORDER_REVIEW"
        task_version: string | number
        subject_version: string
        status: WorkItemStatus
        assignment_mode: "DIRECT" | "POOL"
        owner_role: string
        owner_organization_id: string
        owner_user_id?: string | null
        processing_state: WorkItemProcessingState
        responsibility_actions: readonly WorkItemAllowedAction[]
        domain_allowed_actions: readonly ("APPROVE" | "REJECT")[]
        action_blockers: readonly {
            action: string
            code: string
            message: string
        }[]
    } | null
    created_at: number
}

type BackendBasisLine = {
    procurement_confirmation_line_id: string
    sales_order_submission_line_id: string
    supplier_id: string
    confirmed_quantity: string
    latest_cost_gross: string
    input_tax_rate: string
    expected_delivery_date: string
    product_name?: string | null
    specification?: string | null
    gross_amount: string
}

type BackendBasis = {
    basis_id: string
    sales_order_id: string
    submission_id: string
    supplier_id: string
    supplier_name: string
    payment_term_code: string
    lines: BackendBasisLine[]
    estimated_gross: string
}

type BackendCreateResult = {
    purchase_order_id: string
    purchase_no: string
    lock_version: number
    replayed?: boolean
    reference: string
}

type BackendSaveResult = {
    lock_version: number
    totals: { gross: string; net: string; tax: string }
    reference: string
}

type BackendSubmitResult = {
    purchase_order_id: string
    purchase_no: string
    submission_id: string
    submission_no: string
    work_item_id: string
    task_version: string | number
    subject_version: string
    lock_version: number
    reference: string
}

type BackendReviewResult = {
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    subject_version: string
    review_result: string
    revision_id?: string | null
    revision_no?: number | null
    payable_entry_id?: string | null
    lock_version: number
    reference: string
}

type BackendChangeStartResult = {
    change_id: string
    base_revision_id: string
    base_revision_no: number
    lock_version: number
    reference: string
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

function apiErrorMessage(error: unknown): string {
    if (!isApiError(error)) {
        return getErrorMessage(error, "请求失败")
    }
    const data = error.responseData as { errorMessage?: string } | undefined
    if (
        data &&
        typeof data.errorMessage === "string" &&
        data.errorMessage &&
        data.errorMessage !== "OK"
    ) {
        return data.errorMessage
    }
    return error.message
}

function apiErrorCode(error: unknown): string {
    if (!isApiError(error)) return "REQUEST_FAILED"
    if (error.status === 409) return "CONFLICT"
    if (error.status === 404) return "NOT_FOUND"
    if (error.status === 403) return "FORBIDDEN"
    if (error.status === 422) return "UNPROCESSABLE"
    if (error.kind === "Validation") return "VALIDATION"
    return error.kind.toUpperCase()
}

function formalActionFailure<T>(
    error: unknown,
    idempotencyKey: string,
): FormalActionResponse<T> {
    if (classifyFormalCommandError(error) === "unknown") {
        return {
            status: "unknown",
            message: "处理结果待确认。当前输入已保留，请稍后使用本次操作重试。",
            idempotencyKey,
        }
    }
    return {
        status: "failed",
        message: apiErrorMessage(error),
        code: apiErrorCode(error),
    }
}

function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return new Date(0).toISOString()
    return new Date(secs * 1000).toISOString()
}

/** 前端状态 → 后端状态 */
function toBackendStatus(
    status?: PurchaseOrderStatusFilter,
): string | undefined {
    if (!status || status === "all") return undefined
    switch (status) {
        case "PENDING_REVIEW":
            return "PENDING_FINANCE_REVIEW"
        case "PARTIAL":
            return "PARTIALLY_EXECUTED"
        case "VOID":
            return "VOIDED"
        default:
            return status
    }
}

/** 后端状态 → 前端状态 */
function fromBackendStatus(status: string): PurchaseOrderStatus {
    switch (status) {
        case "PENDING_FINANCE_REVIEW":
            return "PENDING_REVIEW"
        case "PARTIALLY_EXECUTED":
            return "PARTIAL"
        case "VOIDED":
            return "VOID"
        case "DRAFT":
        case "EFFECTIVE":
        case "COMPLETED":
            return status
        default:
            return "DRAFT"
    }
}

function fromBackendReviewStatus(
    status: string,
    orderStatus: PurchaseOrderStatus,
): PurchaseReviewStatus {
    if (orderStatus === "DRAFT" && status !== "REJECTED") {
        // 草稿且未进入审核轨时前端展示 NONE
        if (status === "PENDING" || !status) return "NONE"
    }
    if (status === "PENDING") return "PENDING"
    if (status === "APPROVED") return "APPROVED"
    if (status === "REJECTED") return "REJECTED"
    return "NONE"
}

function progressDisplay(
    code: string,
    kind: "payment" | "invoice" | "fulfillment",
): string {
    const normalized = (code ?? "NONE").toUpperCase()
    if (kind === "payment") {
        if (normalized === "NONE") return "未付"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "已付"
    }
    if (kind === "invoice") {
        if (normalized === "NONE") return "未收"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "完成"
    }
    if (kind === "fulfillment") {
        if (normalized === "NONE") return "未开始"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "完成"
    }
    return code || "—"
}

function paymentTermLabel(code: string): string {
    return (
        PAYMENT_TERM_OPTIONS.find((o) => o.value === code)?.label ??
        (code === "NET-30" ? "货到 30 天" : code || "—")
    )
}

function mapPurchaseType(value: string): PurchaseType {
    if (value === "PHYSICAL" || value === "VIRTUAL" || value === "SERVICE") {
        return value
    }
    return "PHYSICAL"
}

function mapFulfillment(value: string): FulfillmentResponsibility {
    if (
        value === "WAREHOUSE" ||
        value === "SUPPLIER_DIRECT" ||
        value === "ELECTRONIC" ||
        value === "SERVICE"
    ) {
        return value
    }
    return "WAREHOUSE"
}

function deriveAllowedActions(status: PurchaseOrderStatus): string[] {
    const common = ["OPEN_CENTER", "PRINT"]
    if (status === "DRAFT") {
        return [...common, "EDIT", "SUBMIT", "VOID"]
    }
    if (status === "PENDING_REVIEW") {
        // 财务审核只能从服务端 review_work_item 责任投影进入。
        return common
    }
    if (status === "EFFECTIVE" || status === "PARTIAL") {
        return [...common, "FULFILL", "PAY", "START_CHANGE"]
    }
    return common
}

function mapListItem(row: BackendListItem): PurchaseOrderListItem {
    const status = fromBackendStatus(row.status)
    const reviewStatus = fromBackendReviewStatus(row.review_status, status)
    return {
        purchaseOrderId: row.id,
        purchaseNo: row.purchase_no || undefined,
        draftLabel:
            status === "DRAFT"
                ? `草稿 · ${row.purchase_no || row.id.slice(0, 8)}`
                : undefined,
        revisionNo: undefined,
        status,
        statusLabel: PO_STATUS_LABEL[status],
        statusTone: PO_STATUS_TONE[status],
        reviewStatus,
        reviewLabel: REVIEW_STATUS_LABEL[reviewStatus],
        salesOrderId: row.sales_order_id,
        // 缺口：D15 列表不返回 sales_order_no
        salesOrderNo: row.sales_order_id,
        supplierId: row.supplier_id,
        supplierName: row.supplier_name,
        purchaseType: mapPurchaseType(String(row.purchase_type)),
        // 缺口：列表无履约责任
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "",
        paymentTermLabel: "—",
        ownerName: "—",
        grossAmount: row.gross_amount ?? "0",
        netAmount: row.net_amount ?? "0",
        taxAmount: row.tax_amount ?? "0",
        costMasked: false,
        paymentProgress: progressDisplay(row.payment_progress, "payment"),
        invoiceProgress: progressDisplay(row.invoice_progress, "invoice"),
        fulfillmentProgress: progressDisplay(
            row.fulfillment_progress,
            "fulfillment",
        ),
        // 缺口：后端列表无先款门禁
        paymentGate: "NOT_APPLICABLE" as PaymentGateState,
        expectedDate: undefined,
        updatedAt: secsToIso(row.created_at),
        allowedActions: deriveAllowedActions(status),
        actionBlockers: [],
    }
}

function mapCenter(center: BackendCenter): PurchaseOrderCenterView {
    const status = fromBackendStatus(center.status)
    const reviewStatus = fromBackendReviewStatus(center.review_status, status)
    const contentSource =
        center.content_source === "SUBMISSION" ||
        center.content_source === "REVISION"
            ? center.content_source
            : "DRAFT"

    const lines = (center.lines ?? []).map((line) => ({
        lineId: line.line_id,
        lineType:
            line.line_type === "LOGISTICS_FEE"
                ? ("LOGISTICS_FEE" as const)
                : ("ITEM_SERVICE" as const),
        procurementConfirmationLineId:
            line.procurement_confirmation_line_id ?? undefined,
        itemName:
            line.product_name ??
            (line.line_type === "LOGISTICS_FEE" ? "物流费用" : "采购明细"),
        itemSku: line.sku_id ?? undefined,
        quantity: line.quantity ?? undefined,
        unit: line.base_unit_code ?? undefined,
        unitCostGross: line.unit_cost_gross ?? "0",
        inputTaxRate: line.input_tax_rate ?? "0",
        grossAmount: line.gross_amount ?? "0",
        netAmount: line.net_amount ?? "0",
        taxAmount: line.tax_amount ?? "0",
        expectedDeliveryDate: line.expected_delivery_date ?? undefined,
        logisticsFeeReason: undefined,
        salesAllocationLabel: line.sales_order_submission_line_id
            ? `销售行 ${line.sales_order_submission_line_id.slice(0, 8)}`
            : undefined,
    }))

    const fulfillmentLabel = progressDisplay(
        center.fulfillment_progress,
        "fulfillment",
    )

    return {
        identity: {
            purchaseOrderId: center.id,
            purchaseNo: center.purchase_no || undefined,
            draftLabel:
                status === "DRAFT"
                    ? `草稿 · ${center.purchase_no || center.id.slice(0, 8)}`
                    : undefined,
            status,
            statusLabel: PO_STATUS_LABEL[status],
            statusTone: PO_STATUS_TONE[status],
            reviewStatus,
            reviewLabel: REVIEW_STATUS_LABEL[reviewStatus],
            lockVersion: center.version,
            currentSubmissionId: center.current_submission_id ?? undefined,
            currentRevisionId: center.current_revision_id ?? undefined,
            revisionNo: center.revision_no ?? undefined,
            subjectHash: center.current_submission_id ?? undefined,
        },
        header: {
            salesOrderId: center.sales_order_id,
            salesOrderNo: center.sales_order_id,
            supplierId: center.supplier_id,
            supplierSnapshot: center.supplier_name,
            purchaseType: mapPurchaseType(String(center.purchase_type)),
            fulfillmentResponsibility: mapFulfillment(
                String(center.fulfillment_responsibility),
            ),
            paymentTermCode: center.payment_term_code,
            paymentTermLabel: paymentTermLabel(center.payment_term_code),
            ownerName: "—",
            submittedBy: undefined,
            submittedAt: undefined,
            expectedDate: lines.find((l) => l.expectedDeliveryDate)
                ?.expectedDeliveryDate,
            creationBasisId: undefined,
        },
        progress: {
            payment: progressDisplay(center.payment_progress, "payment"),
            invoice: progressDisplay(center.invoice_progress, "invoice"),
            fulfillment: fulfillmentLabel,
            // 缺口：对象中心无 prepayment_gate 投影
            prepaymentGate: {
                state: "NOT_APPLICABLE",
                message: "先款门禁数据未由后端返回",
                required: "0",
                allocated: "0",
                gap: "0",
                updatedAt: secsToIso(center.created_at),
            },
        },
        currentContent: {
            source: contentSource,
            version: center.revision_no ?? center.version,
            subjectHash: center.current_submission_id ?? undefined,
            lines,
            totals: {
                gross: center.totals?.gross ?? "0",
                net: center.totals?.net ?? "0",
                tax: center.totals?.tax ?? "0",
            },
            costMasked: false,
        },
        allocations: (center.allocations ?? []).map((a) => ({
            lineId: a.purchase_order_revision_line_id,
            salesOrderLineLabel: a.sales_order_revision_line_id,
            allocatedQuantity: a.allocated_quantity,
        })),
        payableSummary: undefined,
        fulfillmentSummary: {
            progressLabel: fulfillmentLabel,
            progressTone:
                center.fulfillment_progress === "COMPLETED"
                    ? "success"
                    : center.fulfillment_progress === "PARTIAL"
                      ? "info"
                      : "neutral",
            inboundQty: "—",
            shippedQty: "—",
            remainingQty: "—",
        },
        changes: (center.changes ?? []).map((c) => ({
            changeId: c.change_id,
            label: c.reason || c.change_id,
            statusLabel: c.status,
            tone: "neutral" as const,
            baseRevisionNo: undefined,
        })),
        workflow: [],
        allowedActions: deriveAllowedActions(status),
        actionBlockers: center.review_work_item?.action_blockers ?? [],
        fieldVisibility: {},
        reviewWorkItem:
            center.review_work_item?.work_item_type ===
                "PURCHASE_ORDER_REVIEW" &&
            center.review_work_item.subject_version &&
            center.review_work_item.task_version != null &&
            center.review_work_item.status === "OPEN"
                ? {
                      workItemId: center.review_work_item.work_item_id,
                      workItemType: center.review_work_item.work_item_type,
                      taskVersion: String(center.review_work_item.task_version),
                      subjectVersion: center.review_work_item.subject_version,
                      status: center.review_work_item.status,
                      assignmentMode: center.review_work_item.assignment_mode,
                      ownerRole: center.review_work_item.owner_role,
                      ownerOrganizationId:
                          center.review_work_item.owner_organization_id,
                      ownerUserId:
                          center.review_work_item.owner_user_id ?? undefined,
                      processingState: center.review_work_item.processing_state,
                      responsibilityActions:
                          center.review_work_item.processing_state === "READY"
                              ? (center.review_work_item
                                    .responsibility_actions ?? [])
                              : [],
                      domainAllowedActions:
                          center.review_work_item.processing_state === "READY"
                              ? (center.review_work_item
                                    .domain_allowed_actions ?? [])
                              : [],
                      actionBlockers:
                          center.review_work_item.action_blockers ?? [],
                  }
                : undefined,
    }
}

function mapBasis(basis: BackendBasis): PurchaseCreationBasis {
    return {
        basisId: basis.basis_id,
        salesOrderId: basis.sales_order_id,
        // 缺口：依据无 sales_order_no
        salesOrderNo: basis.sales_order_id,
        salesSubmissionId: basis.submission_id,
        salesSubmissionNo: 0,
        supplierId: basis.supplier_id,
        supplierName: basis.supplier_name,
        // 缺口：依据无 purchase_type / fulfillment_responsibility
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: basis.payment_term_code || "POSTPAY_NET30",
        paymentTermLabel: paymentTermLabel(
            basis.payment_term_code || "POSTPAY_NET30",
        ),
        lines: (basis.lines ?? []).map((line) => ({
            procurementConfirmationLineId:
                line.procurement_confirmation_line_id,
            itemName: line.product_name ?? "确认分行",
            itemSku: line.specification ?? undefined,
            quantity: String(line.confirmed_quantity ?? "0"),
            unit: "",
            unitCostGross: String(line.latest_cost_gross ?? "0"),
            inputTaxRate: String(line.input_tax_rate ?? "0"),
            expectedDeliveryDate: line.expected_delivery_date ?? "",
            salesAllocationLabel: line.sales_order_submission_line_id,
        })),
        estimatedGross: basis.estimated_gross ?? "0",
        consumed: false,
    }
}

function metricStatusParam(
    metric: PurchaseOrderMetricFilter | undefined,
): string | undefined {
    switch (metric) {
        case "draft":
            return "DRAFT"
        case "review":
            return "PENDING_FINANCE_REVIEW"
        case "fulfill":
            // 后端无「待履约」复合筛选；用 EFFECTIVE 近似
            return "EFFECTIVE"
        case "gate_blocked":
            // 缺口：无门禁筛选
            return undefined
        default:
            return undefined
    }
}

async function countByStatus(status?: string): Promise<number> {
    const page = await apiGet<BackendPage<BackendListItem>>(
        "/admin/purchase-orders",
        {
            status,
            page: 1,
            page_size: 1,
        },
    )
    return page.total ?? 0
}

async function buildMetrics(
    basesCount: number,
): Promise<PurchaseOrderListResult["metrics"]> {
    const [all, draft, review, fulfill] = await Promise.all([
        countByStatus(undefined),
        countByStatus("DRAFT"),
        countByStatus("PENDING_FINANCE_REVIEW"),
        countByStatus("EFFECTIVE"),
    ])
    return [
        {
            key: "all",
            label: "全部采购单",
            count: all,
            detail: "当前数据范围",
        },
        {
            key: "pending_create",
            label: "可建单依据",
            count: basesCount,
            detail: "采购二次确认固定结果",
        },
        {
            key: "draft",
            label: "草稿",
            count: draft,
            detail: "可继续编辑",
        },
        {
            key: "review",
            label: "待财务审核",
            count: review,
            detail: "财务闸门",
        },
        {
            key: "fulfill",
            label: "待履约",
            count: fulfill,
            detail: "含门禁阻塞",
        },
        {
            key: "gate_blocked",
            label: "先款门禁阻塞",
            count: 0,
            detail: "后端未投影门禁指标",
        },
    ]
}

// ─── Public API ──────────────────────────────────────────────────────────────

export async function fetchPurchaseOrders(
    query: PurchaseOrderListQuery = {},
): Promise<PurchaseOrderListResult> {
    const pageSize = Math.min(
        Math.max(1, query.pageSize ?? PURCHASE_ORDER_DEFAULT_PAGE_SIZE),
        PURCHASE_ORDER_MAX_PAGE_SIZE,
    )
    const page = Math.max(1, Math.floor(query.page ?? 1))

    // metric 与 status 叠加：metric 优先映射为 status
    const statusFromMetric = metricStatusParam(query.metric)
    const status = statusFromMetric ?? toBackendStatus(query.status)

    // 后端排序白名单仅 created_at / purchase_no；前端 document/amount 等映射
    let sortBy: string | undefined
    if (query.sortBy === "document") sortBy = "purchase_no"
    else if (
        query.sortBy === "owner" ||
        query.sortBy === "amount" ||
        query.sortBy === "source"
    ) {
        // 缺口：不支持的排序列，回落 created_at
        sortBy = "created_at"
    } else if (
        query.sortBy === "created_at" ||
        query.sortBy === "purchase_no"
    ) {
        sortBy = query.sortBy
    }

    if (query.metric === "pending_create") {
        // 列表展示建单依据不是采购单行：返回空列表 + metrics
        const bases = await fetchCreationBases()
        const open = bases.filter((b) => !b.consumed)
        return {
            rows: [],
            total: 0,
            page: 1,
            pageSize,
            metrics: await buildMetrics(open.length),
            freshness: {
                updatedAt: new Date().toISOString(),
                state: "fresh",
            },
        }
    }

    const pageData = await apiGet<BackendPage<BackendListItem>>(
        "/admin/purchase-orders",
        {
            q: query.q,
            status,
            page,
            page_size: pageSize,
            sort_by: sortBy,
            sort_dir: query.sortDir,
        },
    )

    const bases = await fetchCreationBases().catch(
        () => [] as PurchaseCreationBasis[],
    )
    const rows = (pageData.items ?? []).map(mapListItem)

    return {
        rows,
        total: pageData.total ?? rows.length,
        page: pageData.page ?? page,
        pageSize: pageData.page_size ?? pageSize,
        metrics: await buildMetrics(bases.filter((b) => !b.consumed).length),
        freshness: {
            updatedAt: new Date().toISOString(),
            state: "fresh",
        },
    }
}

export async function fetchPurchaseOrderExportData(
    query: PurchaseOrderListQuery = {},
): Promise<PurchaseOrderListItem[]> {
    // 导出：拉大页聚合（后端无独立导出投影）
    const result = await fetchPurchaseOrders({
        ...query,
        page: 1,
        pageSize: PURCHASE_ORDER_MAX_PAGE_SIZE,
    })
    return result.rows
}

export async function fetchPurchaseOrderCenter(
    purchaseOrderId: string,
): Promise<PurchaseOrderCenterView | null> {
    try {
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}`,
        )
        return mapCenter(center)
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }
}

export async function fetchCreationBases(): Promise<
    readonly PurchaseCreationBasis[]
> {
    const items = await apiGet<BackendBasis[]>("/admin/purchase-creation-bases")
    return (items ?? []).map(mapBasis)
}

/**
 * 草稿编辑令牌：后端无独立 draftEditToken 接口。
 * 用当前 lock_version 生成会话内令牌，服务端以 expected_lock_version 做乐观锁。
 */
export async function acquireDraftEditToken(purchaseOrderId: string): Promise<{
    draftEditToken: string
    lockVersion: number
}> {
    const center = await apiGet<BackendCenter>(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}`,
    )
    return {
        draftEditToken: `det:${purchaseOrderId}:${center.version}`,
        lockVersion: center.version,
    }
}

export async function savePurchaseOrderDraft(
    input: SavePurchaseOrderDraftInput & { paymentTermLabel: string },
): Promise<
    FormalActionResponse<{
        lockVersion: number
        draftContentHash: string
        totals: { gross: string; net: string; tax: string }
    }>
> {
    try {
        // 合并当前中心行字段（后端整表替换；前端仅传补丁）
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}`,
        )
        const patchById = new Map(input.lines.map((l) => [l.lineId, l]))

        const lines = (center.lines ?? []).map((line) => {
            const patch = patchById.get(line.line_id)
            const lineType =
                (patch?.lineType ?? line.line_type) === "LOGISTICS_FEE"
                    ? "LOGISTICS_FEE"
                    : "ITEM_SERVICE"
            return {
                line_type: lineType,
                procurement_confirmation_line_id:
                    line.procurement_confirmation_line_id ?? undefined,
                sku_id: line.sku_id ?? undefined,
                sku_revision_id: line.sku_revision_id ?? undefined,
                product_name: line.product_name ?? undefined,
                specification: line.specification ?? undefined,
                quantity: patch?.quantity ?? line.quantity ?? undefined,
                base_unit_code: line.base_unit_code ?? undefined,
                unit_cost_gross:
                    patch?.unitCostGross ?? line.unit_cost_gross ?? undefined,
                input_tax_rate:
                    patch?.inputTaxRate ?? line.input_tax_rate ?? "0",
                expected_delivery_date:
                    line.expected_delivery_date ?? undefined,
                sales_order_submission_line_id:
                    line.sales_order_submission_line_id ?? undefined,
                allocated_quantity: line.allocated_quantity ?? undefined,
                gross_amount:
                    lineType === "LOGISTICS_FEE"
                        ? (patch?.unitCostGross ?? line.gross_amount)
                        : undefined,
            }
        })

        const data = await apiPost<BackendSaveResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/draft`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: input.paymentTermCode,
                lines,
                idempotency_key: input.idempotencyKey,
            },
        )

        return {
            status: "succeeded",
            data: {
                lockVersion: data.lock_version,
                // 后端无 draft_content_hash：用 reference 占位供前端提交前透传
                draftContentHash: data.reference || `v${data.lock_version}`,
                totals: data.totals,
            },
            reference: data.reference || `SAVED-V${data.lock_version}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function submitPurchaseOrderForReview(
    input: SubmitPurchaseOrderInput,
): Promise<
    FormalActionResponse<{
        submissionId: string
        submissionNo: string
        subjectHash: string
        workItemId: string
        taskVersion: string
        subjectVersion: string
        purchaseNo: string
        lockVersion: number
    }>
> {
    try {
        const data = await apiPost<BackendSubmitResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/submit`,
            {
                expected_lock_version: input.expectedLockVersion,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                submissionId: data.submission_id,
                submissionNo: data.submission_no,
                subjectHash: data.submission_id,
                workItemId: data.work_item_id,
                taskVersion: String(data.task_version),
                subjectVersion: data.subject_version,
                purchaseNo: data.purchase_no,
                lockVersion: data.lock_version,
            },
            reference: data.reference || `SUB-${data.submission_no}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function reviewPurchaseOrder(
    input: ReviewPurchaseOrderInput,
): Promise<
    FormalActionResponse<{
        reviewResult: "APPROVED" | "REJECTED"
        revisionId?: string
        revisionNo?: number
        payableOpenAmount?: string
        lockVersion: number
        reference: string
    }>
> {
    try {
        const decision = input.decision
        const data = await apiPost<BackendReviewResult>(
            `/admin/purchase-orders/${encodeURIComponent(decision.purchaseOrderId)}/review-decisions`,
            {
                work_item_id: input.workItemId,
                expected_task_version: input.expectedTaskVersion,
                expected_subject_version: input.expectedSubjectVersion,
                decision: {
                    purchase_order_id: decision.purchaseOrderId,
                    submission_id: decision.submissionId,
                    expected_purchase_order_lock_version:
                        decision.expectedPurchaseOrderLockVersion,
                    review_result: decision.reviewResult,
                    reason_code:
                        decision.reviewResult === "REJECTED"
                            ? decision.reasonCode
                            : undefined,
                    comment: decision.comment,
                },
                idempotency_key: input.idempotencyKey,
            },
        )
        if (
            data.work_item_id !== input.workItemId ||
            data.work_item_status !== "COMPLETED" ||
            data.subject_version !== input.expectedSubjectVersion ||
            data.review_result !== decision.reviewResult
        ) {
            return {
                status: "unknown",
                message:
                    "处理结果待确认。返回结果不完整，请使用本次操作重试或刷新确认。",
                idempotencyKey: input.idempotencyKey,
            }
        }
        return {
            status: "succeeded",
            data: {
                reviewResult:
                    data.review_result === "REJECTED" ? "REJECTED" : "APPROVED",
                revisionId: data.revision_id ?? undefined,
                revisionNo: data.revision_no ?? undefined,
                payableOpenAmount: undefined,
                lockVersion: data.lock_version,
                reference: data.reference,
            },
            reference: data.reference || `REVIEW-V${data.lock_version}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function startPurchaseChange(input: {
    purchaseOrderId: string
    expectedLockVersion: number
    idempotencyKey: string
}): Promise<
    FormalActionResponse<{ changeId: string; baseRevisionNo: number }>
> {
    try {
        const data = await apiPost<BackendChangeStartResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/changes`,
            {
                expected_lock_version: input.expectedLockVersion,
                // 前端契约未传 reason；后端必填
                reason: "采购变更",
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                changeId: data.change_id,
                baseRevisionNo: data.base_revision_no,
            },
            reference: data.reference || `CHANGE-V${data.base_revision_no}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function createPurchaseOrderFromBasis(
    input: CreatePurchaseOrderFromBasisInput,
): Promise<
    FormalActionResponse<{
        purchaseOrderId: string
        draftLabel: string
        lockVersion: number
    }>
> {
    try {
        const bases = await fetchCreationBases()
        const basis = bases.find((b) => b.basisId === input.basisId)

        const data = await apiPost<BackendCreateResult>(
            "/admin/purchase-orders",
            {
                basis_id: input.basisId,
                // 缺口：前端 Create 输入无 purchase_type；依据也未返回，默认 PHYSICAL
                purchase_type: basis?.purchaseType ?? "PHYSICAL",
                payment_term_code:
                    basis?.paymentTermCode && basis.paymentTermCode.length > 0
                        ? basis.paymentTermCode
                        : "NET-30",
                idempotency_key: input.idempotencyKey,
            },
        )

        return {
            status: "succeeded",
            data: {
                purchaseOrderId: data.purchase_order_id,
                draftLabel: data.purchase_no
                    ? `草稿 · ${data.purchase_no}`
                    : data.reference,
                lockVersion: data.lock_version,
            },
            reference: data.reference || data.purchase_no,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}
