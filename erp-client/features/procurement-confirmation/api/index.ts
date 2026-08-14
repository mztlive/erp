/**
 * W07 采购二次确认 · 真实 HTTP API。
 * 契约形状保持 features/procurement-confirmation/types.ts 与 hooks/queries.ts 不变；
 * 后端差异在本文件内适配，缺口登记见 docs/dev-plan/p4-evidence/F4.md。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import {
    listWorkItems,
    mapWorkItemDto,
    type WorkItemDto,
    type WorkItemProjection,
} from "@/features/work-items"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    FormalActionResponse,
    FormalOutcome,
    FulfillmentMode,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
    ProcurementQueueView,
    RejectReasonCode,
    SubmissionOrigin,
} from "@/features/procurement-confirmation/types"

export type QueueFilters = {
    scope: "mine" | "team"
    due?: "active" | "today" | "overdue"
    sort?: "due_at" | "submitted_at" | "priority"
    orderNo?: string
    currentWorkItemId?: string
    queueContextId?: string
}

// ─── Backend DTO shapes (snake_case wire format) ─────────────────────────────

type BackendPage<T> = {
    items: T[]
    total: number
    page?: number
    page_size?: number
}

type BackendConfirmationLine = {
    id: string
    line_no: number
    sales_order_submission_line_id: string
    supplier_id: string
    supplier_offering_revision_id?: string | null
    confirmed_quantity: string
    latest_cost_gross: string
    input_tax_rate: string
    expected_delivery_date: string
    fulfillment_mode: FulfillmentMode | string
    supplier_capability_revision_id: string
}

type BackendConfirmationDetail = {
    id: string
    sales_order_id: string
    submission_id: string
    status: "PENDING" | "APPROVED" | "REJECTED" | string
    handled_by?: string | null
    handled_at?: number | null
    version: number
    created_at: number
    lines: BackendConfirmationLine[]
    work_item?: WorkItemDto | null
    allowed_actions?: Array<"SAVE" | "APPROVE" | "REJECT">
    action_blockers?: Array<{
        action: "SAVE" | "APPROVE" | "REJECT" | string
        code: string
        message: string
    }>
}

type BackendSalesOrderDetail = {
    id: string
    order_no: string
    business_type?: string
    origin_system?: string
    customer_id?: string
    contract_id?: string | null
    owner_user_id?: string
    owner_user_name?: string | null
    submissions?: Array<{
        id: string
        submission_no: number
        status?: string
        customer_name: string
        contract_no?: string | null
        settlement_party_name?: string | null
        payment_term_name: string
        project_name?: string | null
        business_remark?: string | null
        gross_amount?: string
        net_amount?: string
        tax_amount?: string
        submitted_by?: string
        submitted_at?: number
        lines: Array<{
            id: string
            sales_order_line_id?: string
            line_no: number
            item_name_snapshot?: string
            spec_snapshot?: string | null
            sku_id?: string | null
            unit_snapshot?: string | null
            base_unit_code?: string | null
            quantity?: string | null
            unit_price_gross?: string | null
            sales_tax_rate?: string | null
            fulfillment_mode?: string | null
            fulfillment_due_at?: number | null
            gross_amount?: string
        }>
    }>
    working_copy?: {
        gross_amount?: string
        lines?: Array<{
            id: string
            sales_order_line_id?: string
            line_no: number
            item_name_snapshot?: string
            unit_snapshot?: string | null
            quantity?: string | null
            gross_amount?: string
        }>
    } | null
}

type BackendSupplierOffering = {
    sku_id: string
    supplier_id: string
    status: string
    current_revision_id?: string | null
    current_revision_no?: number | null
    dropship_supply_price_gross?: string | null
    bulk_supply_price_gross?: string | null
    input_tax_rate?: string | null
    bulk_minimum_order_quantity?: string | null
    availability_status?: string | null
    available_quantity?: string | null
    freight_amount?: string | null
    service_fee_amount?: string | null
    valid_from?: string | null
    valid_to?: string | null
}

type BackendRecommendation = {
    confirmation_id: string
    policy_version: string
    calculated_at: number
    ready: boolean
    lines: Array<{
        line_no: number
        sales_order_submission_line_id: string
        item_name: string
        sku_id: string
        supplier_id: string
        supplier_name: string
        supplier_offering_revision_id: string
        confirmed_quantity: string
        latest_cost_gross: string
        input_tax_rate: string
        expected_delivery_date: string
        fulfillment_mode: string
        supplier_capability_revision_id: string
        landed_gross: string
        freight_amount?: string | null
        service_fee_amount?: string | null
        recommendation_reason: string
    }>
    purchase_orders: Array<{
        supplier_id: string
        supplier_name: string
        fulfillment_mode: string
        line_count: number
        estimated_gross: string
    }>
    estimated_purchase_gross: string
    sales_gross: string
    estimated_gross_margin: string
    blocking_issues: Array<{
        code: string
        message: string
        sales_order_submission_line_id?: string | null
    }>
    warnings: Array<{
        code: string
        message: string
        sales_order_submission_line_id?: string | null
    }>
}

type BackendSupplierCapability = {
    supplier_id: string
    capability_code: string
    status: string
    current_revision_id?: string | null
    valid_from: string
    valid_to?: string | null
}

type BackendSupplierDetail = {
    capabilities: BackendSupplierCapability[]
}

export type ProcurementSupplyOption = {
    skuId: string
    supplierId: string
    offeringRevisionId: string
    offeringRevisionNo: number
    costGross: string
    bulkCostGross: string
    dropshipCostGross: string
    bulkMinimumOrderQuantity: string
    inputTaxRate: string
    freightAmount: string
    serviceFeeAmount: string
    capabilities: Array<{
        revisionId: string
        label: string
        capabilityCode: string
    }>
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

function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

function priorityToNumber(p?: string | number | null): number {
    if (typeof p === "number") return p
    switch (p) {
        case "urgent":
            return 100
        case "high":
            return 80
        case "normal":
            return 50
        case "low":
            return 20
        default:
            return 50
    }
}

function mapFulfillmentMode(mode: string): FulfillmentMode {
    if (mode === "COMPANY_WAREHOUSE") return "WAREHOUSE"
    if (mode === "ELECTRONIC_DELIVERY") return "ELECTRONIC"
    if (mode === "OFFLINE_SERVICE") return "SERVICE"
    if (
        mode === "WAREHOUSE" ||
        mode === "SUPPLIER_DIRECT" ||
        mode === "ELECTRONIC" ||
        mode === "SERVICE"
    ) {
        return mode
    }
    return "WAREHOUSE"
}

/** 将后端最低成本方案转换为 W07 页面契约。 */
function mapRecommendation(
    recommendation: BackendRecommendation,
): ProcurementRecommendation {
    const mapIssue = (
        issue: BackendRecommendation["blocking_issues"][number],
    ) => ({
        code: issue.code,
        message: issue.message,
        lineId: issue.sales_order_submission_line_id ?? undefined,
    })
    return {
        confirmationId: recommendation.confirmation_id,
        policyVersion: recommendation.policy_version,
        calculatedAt: secsToIso(recommendation.calculated_at),
        ready: recommendation.ready,
        lines: recommendation.lines.map((line) => ({
            lineKey: `recommended-${line.line_no}-${line.supplier_offering_revision_id}`,
            submissionLineId: line.sales_order_submission_line_id,
            supplierId: line.supplier_id,
            supplierName: line.supplier_name,
            offeringRevisionId: line.supplier_offering_revision_id,
            confirmedQuantity: line.confirmed_quantity,
            latestCostGross: line.latest_cost_gross,
            inputTaxRate: line.input_tax_rate,
            expectedDeliveryDate: line.expected_delivery_date,
            fulfillmentMode: mapFulfillmentMode(line.fulfillment_mode),
            capabilityRevisionId: line.supplier_capability_revision_id,
            capabilitySummary: "当前有效供应商能力",
            qualificationStatus: "VALID" as const,
            itemName: line.item_name,
            itemSku: line.sku_id,
            landedGross: line.landed_gross,
            freightAmount: line.freight_amount ?? undefined,
            serviceFeeAmount: line.service_fee_amount ?? undefined,
            recommendationReason: line.recommendation_reason,
        })),
        purchaseOrders: recommendation.purchase_orders.map((order) => ({
            supplierId: order.supplier_id,
            supplierName: order.supplier_name,
            fulfillmentMode: mapFulfillmentMode(order.fulfillment_mode),
            lineCount: order.line_count,
            estimatedGross: order.estimated_gross,
        })),
        estimatedPurchaseGross: recommendation.estimated_purchase_gross,
        salesGross: recommendation.sales_gross,
        estimatedGrossMargin: recommendation.estimated_gross_margin,
        blockingIssues: recommendation.blocking_issues.map(mapIssue),
        warnings: recommendation.warnings.map(mapIssue),
    }
}

function filterSummary(filters: QueueFilters): string {
    const parts = [
        filters.scope === "mine" ? "仅我的" : "团队",
        filters.due === "overdue"
            ? "已超期"
            : filters.due === "today"
              ? "今日到期"
              : "有效全部",
        filters.sort === "priority"
            ? "优先级"
            : filters.sort === "submitted_at"
              ? "提交时间"
              : "截止优先",
    ]
    if (filters.orderNo) parts.push(`单号 ${filters.orderNo}`)
    return parts.join(" · ")
}

function emptyCoverageFromLines(
    lines: readonly ConfirmationLineDraft[],
    submissionLineIds: readonly {
        id: string
        name: string
        required: string
    }[],
): {
    coverageByLine: CoverageByLine[]
    estimatedPurchaseGross: string
    blockingIssues: ProcurementConfirmationTask["decisionSummary"]["blockingIssues"]
    warnings: ProcurementConfirmationTask["decisionSummary"]["warnings"]
} {
    // 覆盖/毛利由服务端裁决；此处仅做展示占位，不重算金额（P4 §2.7）。
    // 缺口：后端详情未返回 decisionSummary / coverageByLine。
    const coverageByLine: CoverageByLine[] = submissionLineIds.map((s) => {
        const confirmedLines = lines.filter((l) => l.submissionLineId === s.id)
        const confirmed =
            confirmedLines.length > 0
                ? confirmedLines.map((l) => l.confirmedQuantity).join("+") ||
                  "0"
                : "0"
        return {
            submissionLineId: s.id,
            itemName: s.name,
            confirmed,
            required: s.required,
            complete: confirmedLines.length > 0,
            gap: confirmedLines.length > 0 ? "0" : s.required,
        }
    })
    return {
        coverageByLine,
        estimatedPurchaseGross: "—",
        blockingIssues: [],
        warnings: [],
    }
}

async function fetchWorkItemsForQueue(
    filters: QueueFilters,
): Promise<WorkItemProjection[]> {
    const page = await listWorkItems({
        scope: filters.scope,
        workItemType: "PROCUREMENT_CONFIRMATION",
        due:
            filters.due === "today" || filters.due === "overdue"
                ? filters.due
                : undefined,
        query: filters.orderNo,
        sort:
            filters.sort === "priority"
                ? "priority_due"
                : filters.sort === "submitted_at"
                  ? "created_desc"
                  : "due_asc",
        queueContextId: filters.queueContextId,
        currentWorkItemId: filters.currentWorkItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 100,
    })
    return page.items.map(mapWorkItemDto)
}

async function fetchConfirmationDetail(
    confirmationId: string,
    workItemId?: string,
): Promise<BackendConfirmationDetail | null> {
    try {
        return await apiGet<BackendConfirmationDetail>(
            `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}`,
            { work_item_id: workItemId },
        )
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }
}

/** 读取后端计算的最低可执行成本采购方案。 */
export async function fetchProcurementRecommendation(
    confirmationId: string,
): Promise<ProcurementRecommendation> {
    const recommendation = await apiGet<BackendRecommendation>(
        `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}/recommendation`,
    )
    return mapRecommendation(recommendation)
}

async function fetchSalesOrderDetail(
    salesOrderId: string,
): Promise<BackendSalesOrderDetail> {
    return apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
    )
}

/** W02 使用的采购确认业务展示摘要，避免把内部对象 ID 直接展示给用户。 */
export type ProcurementWorkItemPresentation = {
    salesOrderNo: string
    customerName: string
    contractNo?: string
    paymentTermName: string
    grossAmount: string
}

/**
 * 解析采购确认待办对应的销售提交快照。
 *
 * @param confirmationId 采购确认批次 ID。
 * @returns 可直接用于待办列表的业务摘要；对象不存在时返回 null。
 */
export async function fetchProcurementWorkItemPresentation(
    confirmationId: string,
): Promise<ProcurementWorkItemPresentation | null> {
    const detail = await fetchConfirmationDetail(confirmationId)
    if (!detail) return null
    const sales = await fetchSalesOrderDetail(detail.sales_order_id)
    const submission = sales.submissions?.find(
        (row) => row.id === detail.submission_id,
    )
    if (!submission) return null
    return {
        salesOrderNo: sales.order_no,
        customerName: submission.customer_name,
        contractNo: submission.contract_no ?? undefined,
        paymentTermName: submission.payment_term_name,
        grossAmount: String(submission.gross_amount ?? "0"),
    }
}

const CAPABILITY_LABEL: Record<string, string> = {
    physical: "实物商品",
    virtual: "虚拟商品",
    offline_service: "线下服务",
    api: "API",
    printing: "印刷",
}

/**
 * 按销售 SKU 批量读取当前有效供给及供应商能力修订。
 *
 * @param skuIds 销售提交行中的公司 SKU 集合。
 * @returns 可用于采购确认分行选择的不可变版本选项。
 */
export const fetchProcurementSupplyOptions = async (
    skuIds: readonly string[],
): Promise<ProcurementSupplyOption[]> => {
    const today = new Date().toISOString().slice(0, 10)
    const uniqueSkuIds = [...new Set(skuIds.filter(Boolean))]
    const offeringPages = await Promise.all(
        uniqueSkuIds.map((skuId) =>
            apiGet<BackendPage<BackendSupplierOffering>>(
                "/admin/supplier-offerings",
                { sku_id: skuId, page: 1, page_size: 100 },
            ),
        ),
    )
    const offerings = offeringPages
        .flatMap((page) => page.items)
        .filter(
            (offering) =>
                offering.status === "ACTIVE" &&
                Boolean(offering.current_revision_id) &&
                offering.availability_status === "AVAILABLE" &&
                (!offering.valid_from || offering.valid_from <= today) &&
                (!offering.valid_to || today <= offering.valid_to) &&
                (offering.available_quantity == null ||
                    Number(offering.available_quantity) > 0),
        )
    const supplierIds = [...new Set(offerings.map((row) => row.supplier_id))]
    const supplierDetails = await Promise.all(
        supplierIds.map(async (supplierId) => ({
            supplierId,
            detail: await apiGet<BackendSupplierDetail>(
                `/admin/suppliers/${encodeURIComponent(supplierId)}`,
            ),
        })),
    )
    const capabilitiesBySupplier = new Map(
        supplierDetails.map(({ supplierId, detail }) => [
            supplierId,
            detail.capabilities
                .filter(
                    (capability) =>
                        capability.status === "active" &&
                        Boolean(capability.current_revision_id) &&
                        capability.valid_from <= today &&
                        (!capability.valid_to || today <= capability.valid_to),
                )
                .map((capability) => ({
                    revisionId: capability.current_revision_id!,
                    label:
                        CAPABILITY_LABEL[capability.capability_code] ??
                        "供应商能力",
                    capabilityCode: capability.capability_code,
                })),
        ]),
    )
    return offerings.map((offering) => ({
        skuId: offering.sku_id,
        supplierId: offering.supplier_id,
        offeringRevisionId: offering.current_revision_id!,
        offeringRevisionNo: offering.current_revision_no ?? 1,
        costGross:
            offering.bulk_supply_price_gross ??
            offering.dropship_supply_price_gross ??
            "",
        bulkCostGross: offering.bulk_supply_price_gross ?? "",
        dropshipCostGross: offering.dropship_supply_price_gross ?? "",
        bulkMinimumOrderQuantity: offering.bulk_minimum_order_quantity ?? "1",
        inputTaxRate: offering.input_tax_rate ?? "",
        freightAmount: offering.freight_amount ?? "0",
        serviceFeeAmount: offering.service_fee_amount ?? "0",
        capabilities: capabilitiesBySupplier.get(offering.supplier_id) ?? [],
    }))
}

function mapConfirmationLines(
    lines: BackendConfirmationLine[],
): ConfirmationLineDraft[] {
    return lines.map((line) => ({
        lineKey: line.id,
        submissionLineId: line.sales_order_submission_line_id,
        supplierId: line.supplier_id,
        supplierName: "供应商名称加载中",
        offeringRevisionId: line.supplier_offering_revision_id ?? "",
        confirmedQuantity: String(line.confirmed_quantity ?? "0"),
        latestCostGross: String(line.latest_cost_gross ?? "0"),
        inputTaxRate: String(line.input_tax_rate ?? "0"),
        expectedDeliveryDate: line.expected_delivery_date ?? "",
        fulfillmentMode: mapFulfillmentMode(String(line.fulfillment_mode)),
        capabilityRevisionId: line.supplier_capability_revision_id ?? "",
        capabilitySummary: "",
        qualificationStatus: line.supplier_capability_revision_id
            ? ("VALID" as const)
            : ("INVALID" as const),
    }))
}

async function projectTask(
    workItem: WorkItemProjection,
    scope: QueueFilters["scope"],
): Promise<ProcurementConfirmationTask | null> {
    if (workItem.status !== "OPEN") {
        return null
    }

    const confirmationId = workItem.businessObjectId
    const detail = await fetchConfirmationDetail(
        confirmationId,
        workItem.workItemId,
    )
    if (!detail) {
        throw new Error(
            `任务 ${workItem.workItemId} 绑定的采购确认 ${confirmationId} 不存在；已禁止隐藏任务或继续处理`,
        )
    }
    if (!detail.work_item) {
        throw new Error(
            `采购确认 ${confirmationId} 未返回 actor-specific 正式任务；已禁止从队列待办推导领域动作`,
        )
    }
    const projectedWorkItem = mapWorkItemDto(detail.work_item)
    if (
        projectedWorkItem.workItemId !== workItem.workItemId ||
        projectedWorkItem.businessObjectId !== confirmationId
    ) {
        throw new Error("服务端返回的 W07 正式任务与队列身份不一致")
    }
    if (detail.status === "APPROVED" || detail.status === "REJECTED") {
        return null
    }

    const sales = await fetchSalesOrderDetail(detail.sales_order_id)
    const submission = sales.submissions?.find(
        (row) => row.id === detail.submission_id,
    )
    if (!submission) {
        throw new Error(
            `采购确认 ${detail.id} 未取得其绑定的不可变销售提交 ${detail.submission_id}；已禁止使用其它提交继续处理`,
        )
    }

    const confLines = mapConfirmationLines(detail.lines ?? [])

    const submissionLines = submission.lines.map((line) => ({
        submissionLineId: line.id,
        itemName: line.item_name_snapshot ?? `行 ${line.line_no}`,
        itemSku: line.sku_id ?? "",
        specification: line.spec_snapshot ?? undefined,
        committedQuantity: String(line.quantity ?? "0"),
        unit: line.base_unit_code ?? line.unit_snapshot ?? "",
        requestedDeliveryDate:
            secsToIso(line.fulfillment_due_at).slice(0, 10) || "—",
        unitPriceGross: line.unit_price_gross ?? undefined,
        fulfillmentMode: line.fulfillment_mode
            ? mapFulfillmentMode(line.fulfillment_mode)
            : undefined,
        salesTaxRate: line.sales_tax_rate ?? undefined,
        salesAmountGross: String(line.gross_amount ?? "0"),
    }))

    const coverage = emptyCoverageFromLines(
        confLines,
        submissionLines.map((s) => ({
            id: s.submissionLineId,
            name: s.itemName,
            required: s.committedQuantity,
        })),
    )

    const origin: SubmissionOrigin = "INITIAL"

    return {
        workItemId: projectedWorkItem.workItemId,
        taskVersion: projectedWorkItem.taskVersion,
        responsibilityScope: scope,
        status: projectedWorkItem.status,
        assignmentMode: projectedWorkItem.assignmentMode,
        ownerUser: projectedWorkItem.ownerUser,
        priority: priorityToNumber(projectedWorkItem.priority),
        dueAt:
            secsToIso(projectedWorkItem.dueAt) ||
            secsToIso(projectedWorkItem.createdAt),
        impactSummary: projectedWorkItem.impactSummary,
        subjectVersion: projectedWorkItem.subjectVersion,
        subjectHash: detail.submission_id,
        salesSubmission: {
            salesOrderId: detail.sales_order_id,
            salesOrderNo: sales.order_no,
            submissionId: detail.submission_id,
            submissionNo: submission.submission_no,
            subjectHash: detail.submission_id,
            subjectHashSummary: (detail.submission_id ?? "").slice(0, 12),
            submittedAt:
                secsToIso(submission.submitted_at) ||
                secsToIso(detail.created_at),
            submittedByLabel:
                submission.submitted_by === sales.owner_user_id
                    ? (sales.owner_user_name ?? "销售提交人")
                    : "销售提交人",
            customerSnapshot: submission.customer_name,
            contractId: sales.contract_id ?? undefined,
            contractSnapshot: submission.contract_no ?? undefined,
            settlementPartySnapshot:
                submission.settlement_party_name ?? undefined,
            paymentTermLabel: submission.payment_term_name,
            projectName: submission.project_name ?? undefined,
            businessRemark: submission.business_remark ?? undefined,
            grossAmount: String(
                submission.gross_amount ??
                    sales.working_copy?.gross_amount ??
                    "0",
            ),
            netAmount: submission.net_amount,
            taxAmount: submission.tax_amount,
            origin,
            lines: submissionLines,
        },
        confirmation: {
            confirmationId: detail.id,
            status: "PENDING",
            editVersion: detail.version,
            lines: confLines,
        },
        decisionSummary: {
            coverageByLine: coverage.coverageByLine,
            estimatedPurchaseGross: coverage.estimatedPurchaseGross,
            estimatedMargin: undefined,
            marginDelta: undefined,
            blockingIssues: coverage.blockingIssues,
            warnings: coverage.warnings,
        },
        allowedActions: [
            ...projectedWorkItem.allowedActions.filter((action) =>
                ["START_PROCESSING", "RELEASE_TO_TEAM", "REASSIGN"].includes(
                    action,
                ),
            ),
            ...(detail.allowed_actions ?? []),
        ],
        actionBlockers: [
            ...(detail.action_blockers ?? []),
            ...projectedWorkItem.actionBlockers.map((message) => ({
                action: "PROCESS_TASK",
                code: "WORK_ITEM_ACTION_BLOCKED",
                message,
            })),
        ],
        riskLabel: projectedWorkItem.ownerUser ? "处理中" : "团队待处理",
        riskTone: projectedWorkItem.ownerUser ? "info" : "warning",
        riskDescription: projectedWorkItem.impactSummary,
    }
}

// ─── Public API (stable signatures) ──────────────────────────────────────────

export async function fetchProcurementQueue(
    filters: QueueFilters,
): Promise<ProcurementQueueView> {
    const workItems = await fetchWorkItemsForQueue(filters)

    const projected = (
        await Promise.all(workItems.map((wi) => projectTask(wi, filters.scope)))
    ).filter((t): t is ProcurementConfirmationTask => t != null)

    const tasks = projected

    const queueContextId =
        filters.queueContextId ??
        `queue:procurement-confirmation:${filters.scope}`

    let position = 0
    let current = tasks[0]
    if (filters.currentWorkItemId) {
        const idx = tasks.findIndex(
            (t) => t.workItemId === filters.currentWorkItemId,
        )
        if (idx >= 0) {
            position = idx
            current = tasks[idx]
        }
    }

    const emptyReason =
        tasks.length === 0
            ? projected.length === 0 && workItems.length === 0
                ? "NO_TASKS"
                : "FILTER_NO_RESULT"
            : undefined

    return {
        preferences: {
            autoNextDefault: true,
        },
        context: {
            queueContextId,
            position: tasks.length === 0 ? 0 : position + 1,
            total: tasks.length,
            currentWorkItemId: current?.workItemId,
            previousWorkItemId: tasks[position - 1]?.workItemId,
            nextWorkItemId: tasks[position + 1]?.workItemId,
            filterSummary: filterSummary(filters),
            queueContextUpdatedAt: new Date().toISOString(),
        },
        tasks,
        current,
        emptyReason,
    }
}

export async function saveProcurementConfirmation(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    confirmationId: string
    submissionId: string
    expectedEditVersion: number
    lines: ConfirmationLineDraft[]
    idempotencyKey: string
}): Promise<{ editVersion: number; taskVersion: string }> {
    const body = {
        work_item_id: input.workItemId,
        expected_task_version: input.expectedTaskVersion,
        expected_subject_version: input.expectedSubjectVersion,
        action: {
            confirmation_id: input.confirmationId,
            submission_id: input.submissionId,
            expected_edit_version: input.expectedEditVersion,
            lines: input.lines.map((line, index) => ({
                line_no: index + 1,
                sales_order_submission_line_id: line.submissionLineId,
                supplier_id: line.supplierId,
                supplier_offering_revision_id: line.offeringRevisionId,
                confirmed_quantity: line.confirmedQuantity,
                latest_cost_gross: line.latestCostGross,
                input_tax_rate: line.inputTaxRate,
                expected_delivery_date: line.expectedDeliveryDate,
                fulfillment_mode: line.fulfillmentMode,
                supplier_capability_revision_id: line.capabilityRevisionId,
            })),
        },
        idempotency_key: input.idempotencyKey,
    }

    const detail = await apiPut<{
        edit_version: number
        task_version: string | number
    }>(
        `/admin/procurement-confirmations/${encodeURIComponent(input.confirmationId)}/lines`,
        body,
    )
    if (
        !Number.isInteger(detail.edit_version) ||
        (typeof detail.task_version !== "string" &&
            typeof detail.task_version !== "number")
    ) {
        throw new Error(
            "保存接口未返回新的 editVersion 与 taskVersion；当前处理器尚未满足强类型保存合同",
        )
    }
    return {
        editVersion: detail.edit_version,
        taskVersion: String(detail.task_version),
    }
}

export async function completeProcurementDecision(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    idempotencyKey: string
    decision:
        | {
              reviewResult: "APPROVED"
              confirmationId: string
              submissionId: string
              expectedConfirmationEditVersion: number
              salesOrderId: string
              salesOrderNo: string
              subjectHash: string
          }
        | {
              reviewResult: "REJECTED"
              confirmationId: string
              submissionId: string
              expectedConfirmationEditVersion: number
              salesOrderId: string
              salesOrderNo: string
              subjectHash: string
              rejectReasonCode: RejectReasonCode
              comment: string
          }
}): Promise<FormalActionResponse> {
    try {
        const data = await apiPost<{
            work_item_id: string
            work_item_status: "COMPLETED"
            business_result:
                | {
                      outcome: "APPROVED_AND_SALES_EFFECTIVE"
                      procurement_confirmation_id: string
                      sales_order_id: string
                      submission_id: string
                      sales_order_revision_id: string
                      receivable_account_id: string
                      procurement_creation_basis_id: string
                  }
                | {
                      outcome: "REJECTED_TO_SALES"
                      procurement_confirmation_id: string
                      sales_order_id: string
                      rejected_submission_id: string
                      workflow_action_id: string
                      next_sales_resolutions: readonly [
                          "RESUBMIT_CHANGED_TERMS",
                          "REQUEST_LOW_MARGIN_ACCEPTANCE",
                          "VOID_AFTER_REJECTION",
                      ]
                      successor_work_item_id?: string | null
                  }
        }>(
            `/admin/procurement-confirmations/${encodeURIComponent(input.decision.confirmationId)}/decisions`,
            {
                work_item_id: input.workItemId,
                expected_task_version: input.expectedTaskVersion,
                expected_subject_version: input.expectedSubjectVersion,
                decision:
                    input.decision.reviewResult === "APPROVED"
                        ? {
                              review_result: "APPROVED",
                              confirmation_id: input.decision.confirmationId,
                              submission_id: input.decision.submissionId,
                              expected_confirmation_edit_version:
                                  input.decision
                                      .expectedConfirmationEditVersion,
                          }
                        : {
                              review_result: "REJECTED",
                              confirmation_id: input.decision.confirmationId,
                              submission_id: input.decision.submissionId,
                              expected_confirmation_edit_version:
                                  input.decision
                                      .expectedConfirmationEditVersion,
                              reject_reason_code:
                                  input.decision.rejectReasonCode,
                              comment: input.decision.comment,
                          },
                idempotency_key: input.idempotencyKey,
            },
        )
        if (input.decision.reviewResult === "APPROVED") {
            const business = data.business_result
            if (
                data.work_item_status !== "COMPLETED" ||
                business?.outcome !== "APPROVED_AND_SALES_EFFECTIVE" ||
                !business.procurement_creation_basis_id
            ) {
                return {
                    status: "failed",
                    code: "INCOMPLETE_FORMAL_RESULT",
                    message:
                        "任务完成记录或采购创建依据不完整；当前结果不能按成功展示",
                }
            }
            const outcome: FormalOutcome = {
                kind: "APPROVED_AND_SALES_EFFECTIVE",
                procurementConfirmationId: business.procurement_confirmation_id,
                salesOrderId: business.sales_order_id,
                salesOrderNo: input.decision.salesOrderNo,
                submissionId: business.submission_id,
                subjectHash: input.decision.subjectHash,
                salesOrderRevisionId: business.sales_order_revision_id,
                receivableAccountId: business.receivable_account_id,
                procurementCreationBasisId:
                    business.procurement_creation_basis_id,
                reference: business.procurement_creation_basis_id,
            }
            return { status: "succeeded", outcome }
        }

        const business = data.business_result
        if (
            data.work_item_status !== "COMPLETED" ||
            business?.outcome !== "REJECTED_TO_SALES" ||
            business.successor_work_item_id != null ||
            !business.next_sales_resolutions.includes(
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
            )
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message:
                    "驳回结果不完整，或出现了不应存在的后续任务；当前结果不能按成功展示",
            }
        }
        const outcome: FormalOutcome = {
            kind: "REJECTED_TO_SALES",
            procurementConfirmationId: business.procurement_confirmation_id,
            salesOrderId: business.sales_order_id,
            salesOrderNo: input.decision.salesOrderNo,
            rejectedSubmissionId: business.rejected_submission_id,
            rejectedSubjectHash: input.decision.subjectHash,
            workflowActionId: business.workflow_action_id,
            nextSalesResolutions: [
                "RESUBMIT_CHANGED_TERMS",
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
                "VOID_AFTER_REJECTION",
            ],
            reference: business.workflow_action_id,
            rejectReasonCode: input.decision.rejectReasonCode,
            comment: input.decision.comment,
        }
        return { status: "succeeded", outcome }
    } catch (error) {
        if (
            isApiError(error) &&
            (error.kind === "Network" || error.kind === "Parse")
        ) {
            return {
                status: "unknown",
                idempotencyKey: input.idempotencyKey,
                message:
                    "请求结果尚未确认；请按操作号查询处理结果，确认前不得再次提交或打开下一项",
            }
        }
        return {
            status: "failed",
            message: apiErrorMessage(error),
            code: apiErrorCode(error),
        }
    }
}
