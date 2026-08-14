/**
 * W26 供应商订单 · 真实 HTTP API
 * 路径：/admin/supplier-fulfillment-orders、/admin/work-items、/admin/background-jobs
 * 后端视图较精简：缺失字段以安全默认值适配并登记 backend_gap。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
    AfterSalesActionInput,
    AfterSalesActionResult,
    CancelStatus,
    CompleteSupplierOrderTaskInput,
    CompleteSupplierOrderTaskResult,
    ExportCommand,
    ExportJobResult,
    FormalActionResponse,
    NoteInput,
    QueryResultData,
    QueryResultInput,
    RefundStatus,
    ReplayInput,
    ReplayResultData,
    RevealAddressInput,
    RevealAddressResult,
    SupplierFulfillmentStatus,
    SupplierOrderDetailView,
    SupplierOrderListQuery,
    SupplierOrderListResult,
    SupplierOrderListRow,
    SupplierOrderMetric,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    CANCEL_STATUS_TONE,
    FULFILLMENT_STATUS_LABEL,
    FULFILLMENT_STATUS_TONE,
    REFUND_STATUS_LABEL,
    REFUND_STATUS_TONE,
} from "@/features/supplier-orders/types"
import { mapWorkItemDto, type WorkItemDto } from "@/features/work-items/types"

const PAYMENT_OCCURRED_NOTICE =
    "商城支付已发生。供应商履约结果独立记录，不得用取消/退款折入履约主状态。"
const PERMISSION_VERSION = "server"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendOrder = {
    id: string
    fulfillment_order_no: string
    mall_order_id: string
    supplier_id: string
    connection_id: string
    split_no: number
    fulfillment_status: string
    cancel_status: string
    refund_status: string
    external_order_no?: string | null
    submitted_at?: number | null
    accepted_at?: number | null
    completed_at?: number | null
    version: number
    created_at: number
}

type BackendItem = {
    id: string
    supplier_fulfillment_order_id: string
    mall_order_item_id: string
    supplier_offering_revision_id: string
    supplier_sku_code_snapshot: string
    supplier_product_code_snapshot?: string | null
    quantity: string
    unit_cost_snapshot_gross: string
    cost_snapshot_total_gross: string
    input_tax_rate: string
}

type BackendStatusHistory = {
    id: string
    previous_status: string
    new_status: string
    supplier_status_version: string
    occurred_at: number
    received_at: number
    external_event_id: string
    source_type: string
    created_at: number
}

type BackendAction = {
    id: string
    supplier_fulfillment_order_id: string
    action_type: string
    after_sales_request_id?: string | null
    status: string
    external_request_id?: string | null
    request_summary?: string | null
    response_summary?: string | null
    attempt_count: number
    created_at: number
}

type BackendDetail = {
    order: BackendOrder
    items: BackendItem[]
    status_history: BackendStatusHistory[]
    actions: BackendAction[]
    refund_facts: Array<{
        id: string
        supplier_fulfillment_order_id: string
        external_refund_no: string
        refund_amount: string
        refunded_at: number
    }>
    supplier_name?: string | null
    mall_order_no?: string | null
    address: {
        masked?: string | null
        can_reveal: boolean
        blocker_code?: string | null
        blocker_message?: string | null
    }
    work_item?: WorkItemDto | null
    target_supplier_action_id?: string | null
    last_investigation?: BackendInvestigationResult["evidence"] | null
    allowed_actions?: Array<
        "QUERY_RESULT" | "REPLAY" | "CONFIRM_VERIFIED_TERMINAL_RESULT"
    >
    action_blockers?: BackendInvestigationResult["action_blockers"]
}

type BackendSubmitResult = {
    action: BackendAction
    lines: unknown[]
    order: BackendOrder
}

type BackendBackgroundJob = {
    id: string
    job_no: string
    status: string
    result_expires_at?: number | null
}

type BackendInvestigationResult = {
    result_status: "SUCCEEDED" | "UNKNOWN" | "BLOCKED"
    message: string
    operation_id: string
    evidence: {
        evidence_id: string
        target_supplier_action_id: string
        outcome: "VERIFIED_TERMINAL" | "VERIFIED_NO_RESULT" | "RESULT_UNKNOWN"
        recorded_at: number
        can_safe_retry: boolean
        external_order_no?: string | null
        summary: string
        verified_supplier_action_result_id?: string | null
        verified_resolution?:
            | "ORDER_ACCEPTED"
            | "ORDER_REJECTED"
            | "ORDER_COMPLETED"
            | "CANCELED"
            | "REFUNDED"
            | null
    }
    order: BackendOrder
    work_item?: {
        id: string
        status: "OPEN"
        task_version: string | number
    } | null
    allowed_actions: string[]
    action_blockers: Array<{
        action: string
        code: string
        message: string
        destination_workspace_id?: string | null
    }>
}

type BackendTaskCompletionResult = {
    operation_id: string
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    order_lock_version: number
    resolution:
        | "ORDER_ACCEPTED"
        | "ORDER_REJECTED"
        | "ORDER_COMPLETED"
        | "CANCELED"
        | "REFUNDED"
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

function asFulfillment(raw: string): SupplierFulfillmentStatus {
    const u = raw.toUpperCase() as SupplierFulfillmentStatus
    const allowed: SupplierFulfillmentStatus[] = [
        "RECEIVED",
        "SUBMITTING",
        "ACCEPTED",
        "REJECTED",
        "RESULT_UNKNOWN",
        "FULFILLING",
        "SHIPPED",
        "COMPLETED",
        "EXCEPTION",
    ]
    return allowed.includes(u) ? u : "RECEIVED"
}

function asCancel(raw: string): CancelStatus {
    const u = raw.toUpperCase() as CancelStatus
    const allowed: CancelStatus[] = [
        "NONE",
        "CANCEL_PENDING",
        "CANCELED",
        "FAILED",
        "MANUAL",
    ]
    return allowed.includes(u) ? u : "NONE"
}

function asRefund(raw: string): RefundStatus {
    const u = raw.toUpperCase() as RefundStatus
    const allowed: RefundStatus[] = [
        "NONE",
        "REFUND_PENDING",
        "PARTIAL",
        "REFUNDED",
        "REFUND_FAILED",
        "MANUAL",
    ]
    return allowed.includes(u) ? u : "NONE"
}

function priorityOf(status: SupplierFulfillmentStatus): number {
    switch (status) {
        case "RESULT_UNKNOWN":
            return 100
        case "EXCEPTION":
        case "REJECTED":
            return 90
        case "SUBMITTING":
        case "RECEIVED":
            return 70
        default:
            return 10
    }
}

function mapListRow(o: BackendOrder): SupplierOrderListRow {
    const fulfillment = asFulfillment(o.fulfillment_status)
    const cancel = asCancel(o.cancel_status)
    const refund = asRefund(o.refund_status)
    const lastBusinessAt =
        tsToIso(o.completed_at) ||
        tsToIso(o.accepted_at) ||
        tsToIso(o.submitted_at) ||
        tsToIso(o.created_at)

    return {
        orderId: o.id,
        orderNo: o.fulfillment_order_no,
        mallOrderId: o.mall_order_id,
        mallOrderNo: "",
        supplierId: o.supplier_id,
        supplierName: "",
        externalOrderNo: o.external_order_no ?? undefined,
        fulfillmentStatus: fulfillment,
        fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
        fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
        cancelStatus: cancel,
        cancelLabel: CANCEL_STATUS_LABEL[cancel],
        cancelTone: CANCEL_STATUS_TONE[cancel],
        refundStatus: refund,
        refundLabel: REFUND_STATUS_LABEL[refund],
        refundTone: REFUND_STATUS_TONE[refund],
        paidAt: tsToIso(o.created_at),
        updatedAt: lastBusinessAt,
        lastBusinessAt,
        itemCount: 0,
        allowedActions: ["OPEN_CENTER", "NOTE"],
        actionBlockers: [
            {
                action: "VIEW_SUPPLIER_NAME",
                code: "SUPPLIER_NAME_NOT_PROJECTED_IN_LIST",
                message: "列表接口未返回权威供应商名称，禁止以 ID 伪装名称",
            },
            {
                action: "VIEW_MALL_ORDER_NO",
                code: "MALL_ORDER_NO_NOT_PROJECTED_IN_LIST",
                message: "列表接口未返回权威商城订单号",
            },
        ],
        priority: priorityOf(fulfillment),
    }
}

function emptyMetrics(): SupplierOrderMetric[] {
    return [
        {
            key: "pending_submit",
            label: "待提交",
            value: 0,
            fulfillmentStatuses: ["RECEIVED", "SUBMITTING"],
        },
        {
            key: "result_unknown",
            label: "结果未知",
            value: 0,
            fulfillmentStatus: "RESULT_UNKNOWN",
        },
        {
            key: "exception",
            label: "履约异常",
            value: 0,
            fulfillmentStatuses: ["EXCEPTION", "REJECTED"],
        },
        {
            key: "aftersale",
            label: "售后待处理",
            value: 0,
            aftersalePending: true,
        },
        { key: "all", label: "全部订单", value: 0, view: "all" },
    ]
}

function filterSummary(query: SupplierOrderListQuery, total: number): string {
    const parts: string[] = []
    if (query.view === "actionable") parts.push("可操作")
    else if (query.view === "recent_completed") parts.push("最近完成")
    else parts.push("全部")
    if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
    if (query.supplierId) parts.push(query.supplierId)
    if (query.fulfillmentStatuses?.length) {
        parts.push(
            query.fulfillmentStatuses
                .map((s) => FULFILLMENT_STATUS_LABEL[s])
                .join("/"),
        )
    }
    parts.push(`${total} 条`)
    return parts.join(" · ")
}

function mapFormalWorkItem(item: ReturnType<typeof mapWorkItemDto>) {
    return {
        workItemId: item.workItemId,
        taskVersion: item.taskVersion,
        workItemType: item.workItemType as
            | "INTEGRATION_RESULT_UNKNOWN"
            | "BUSINESS_EXCEPTION",
        businessObjectType: "SUPPLIER_FULFILLMENT_ORDER" as const,
        businessObjectId: item.businessObjectId,
        subjectVersion: item.subjectVersion,
        assignmentMode: item.assignmentMode,
        processingState: item.processingState,
        ownerUser: item.ownerUser,
        allowedTaskActions: item.allowedActions,
        actionBlockers: item.actionBlockers,
        workItemStatus: item.status,
    }
}

function mapDetail(d: BackendDetail): SupplierOrderDetailView {
    const o = d.order
    const fulfillment = asFulfillment(o.fulfillment_status)
    const cancel = asCancel(o.cancel_status)
    const refund = asRefund(o.refund_status)
    const formalTask = d.work_item ? mapWorkItemDto(d.work_item) : undefined
    const investigation = d.last_investigation

    return {
        order: {
            id: o.id,
            orderNo: o.fulfillment_order_no,
            mallOrderId: o.mall_order_id,
            mallOrderNo: d.mall_order_no ?? "",
            paidAt: tsToIso(o.created_at),
            paymentFactKey: "",
            fulfillmentChain: "ERP_AUTOMATED",
            supplierId: o.supplier_id,
            supplierName: d.supplier_name ?? "",
            connectionCode: o.connection_id,
            connectionEnvironment: "production",
            supplyVersion: "",
            publicationVersion: "",
            externalOrderNo: o.external_order_no ?? undefined,
            fulfillmentStatus: fulfillment,
            fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
            fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
            cancelStatus: cancel,
            cancelLabel: CANCEL_STATUS_LABEL[cancel],
            cancelTone: CANCEL_STATUS_TONE[cancel],
            refundStatus: refund,
            refundLabel: REFUND_STATUS_LABEL[refund],
            refundTone: REFUND_STATUS_TONE[refund],
            lockVersion: o.version,
            paymentOccurredNotice: PAYMENT_OCCURRED_NOTICE,
        },
        items: (d.items ?? []).map((it) => ({
            itemId: it.id,
            mallLineId: it.mall_order_item_id,
            productName:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            skuCode: it.supplier_sku_code_snapshot,
            quantity: String(it.quantity),
            unit: "件",
            supplierProductId:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            supplierProductName:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            publicationVersion: "",
            supplyVersion: it.supplier_offering_revision_id,
            unitCostGross: String(it.unit_cost_snapshot_gross),
            unitCostNet: null,
            inputTaxRate: String(it.input_tax_rate),
            snapshotImmutable: true as const,
        })),
        logistics: {
            acceptedAt: tsToIso(o.accepted_at) || undefined,
            shippedAt: undefined,
            completedAt: tsToIso(o.completed_at) || undefined,
        },
        statusHistory: (d.status_history ?? []).map((h) => ({
            id: h.id,
            at: tsToIso(h.occurred_at),
            track: "fulfillment" as const,
            fromLabel:
                FULFILLMENT_STATUS_LABEL[asFulfillment(h.previous_status)] ??
                h.previous_status,
            toLabel:
                FULFILLMENT_STATUS_LABEL[asFulfillment(h.new_status)] ??
                h.new_status,
            source: h.source_type,
        })),
        afterSales: [],
        costs: {
            cumulativeCostGross: String(
                d.items?.[0]?.cost_snapshot_total_gross ?? null,
            ),
            cumulativeCostNet: null,
            costSource: "下单成本快照",
            costVariance: null,
        },
        actions: (d.actions ?? []).map((a) => ({
            actionId: a.id,
            actionType:
                (a.action_type as SupplierOrderDetailView["actions"][number]["actionType"]) ||
                "PLACE",
            actionLabel: a.action_type,
            at: tsToIso(a.created_at),
            actor: "系统",
            outcomeLabel: a.status,
            outcomeTone: "neutral" as const,
            idempotencyKeyTail: a.external_request_id
                ? `…${a.external_request_id.slice(-6)}`
                : "—",
            attemptCount: a.attempt_count,
            operationId: a.id,
        })),
        address: {
            masked: d.address.masked ?? "—",
            phoneMasked: "—",
            recipientMasked: "—",
            canReveal: d.address.can_reveal,
        },
        workItem: formalTask ? mapFormalWorkItem(formalTask) : undefined,
        workItemBlocker: undefined,
        lastInvestigation: investigation
            ? {
                  evidenceId: investigation.evidence_id,
                  targetSupplierActionId:
                      investigation.target_supplier_action_id,
                  outcome: investigation.outcome,
                  outcomeLabel: investigation.outcome,
                  recordedAt: tsToIso(investigation.recorded_at),
                  canSafeRetry: investigation.can_safe_retry,
                  externalOrderNo: investigation.external_order_no ?? undefined,
                  summary: investigation.summary,
                  verifiedSupplierActionResultId:
                      investigation.verified_supplier_action_result_id ??
                      undefined,
                  verifiedResolution:
                      investigation.verified_resolution ?? undefined,
              }
            : undefined,
        placeActionId: d.target_supplier_action_id ?? "",
        allowedActions: ["OPEN_CENTER", "NOTE", ...(d.allowed_actions ?? [])],
        actionBlockers: (d.action_blockers ?? []).map((blocker) => ({
            action: blocker.action,
            code: blocker.code,
            message: blocker.message,
        })),
        freshness: {
            updatedAt: tsToIso(o.created_at),
            state: "fresh",
        },
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchSupplierOrders(
    query: SupplierOrderListQuery,
): Promise<SupplierOrderListResult> {
    const now = new Date().toISOString()
    const pageRes = await apiGet<Page<BackendOrder>>(
        "/admin/supplier-fulfillment-orders",
        {
            page: query.page,
            page_size: query.pageSize,
            supplier_id: query.supplierId,
            fulfillment_status: query.fulfillmentStatuses?.[0],
            cancel_status: query.cancelStatuses?.[0],
            refund_status: query.refundStatuses?.[0],
            external_order_no: query.q?.trim() || undefined,
            sort_by:
                query.sortBy === "lastBusinessAt" ? "created_at" : "created_at",
            sort_dir: query.sortDir ?? "desc",
        },
    )

    let rows = (pageRes.items ?? []).map((o) => mapListRow(o))

    // 客户端视图投影（后端未提供 view/actionable 筛选）
    if (query.view === "actionable") {
        rows = rows.filter(
            (r) =>
                r.fulfillmentStatus === "RESULT_UNKNOWN" ||
                r.fulfillmentStatus === "EXCEPTION" ||
                r.fulfillmentStatus === "REJECTED" ||
                r.fulfillmentStatus === "SUBMITTING" ||
                r.fulfillmentStatus === "RECEIVED" ||
                r.cancelStatus === "FAILED" ||
                r.cancelStatus === "MANUAL" ||
                r.cancelStatus === "CANCEL_PENDING" ||
                r.refundStatus === "REFUND_FAILED" ||
                r.refundStatus === "MANUAL" ||
                r.refundStatus === "REFUND_PENDING",
        )
    } else if (query.view === "recent_completed") {
        rows = rows.filter((r) => r.fulfillmentStatus === "COMPLETED")
    }

    return {
        rows,
        pageInfo: {
            page: pageRes.page ?? query.page,
            pageSize: pageRes.page_size ?? query.pageSize,
            total: pageRes.total ?? rows.length,
        },
        metrics: emptyMetrics(),
        permissionVersion: PERMISSION_VERSION,
        sourceAsOf: now,
        queriedAt: now,
        filterSummary: filterSummary(query, pageRes.total ?? rows.length),
    }
}

export async function fetchSupplierOrderDetail(input: {
    orderId: string
    workItemId?: string
}): Promise<SupplierOrderDetailView> {
    const detail = await apiGet<BackendDetail>(
        `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}`,
        { work_item_id: input.workItemId },
    )
    return mapDetail(detail)
}

/**
 * 查询原结果：后端集成在 integration_ops 错误任务上；
 * 履约订单域无独立 QUERY 端点 → 返回 blocked 并指向 W29。
 */
export async function querySupplierResult(
    input: QueryResultInput,
): Promise<FormalActionResponse<QueryResultData>> {
    return submitInvestigation(input) as Promise<
        FormalActionResponse<QueryResultData>
    >
}

/**
 * 安全重发：后端无独立 REPLAY 端点（在 integration_ops error-task replay）。
 */
export async function replaySupplierOrder(
    input: ReplayInput,
): Promise<FormalActionResponse<ReplayResultData>> {
    return submitInvestigation(input) as Promise<
        FormalActionResponse<ReplayResultData>
    >
}

async function submitInvestigation(
    input: QueryResultInput | ReplayInput,
): Promise<FormalActionResponse<QueryResultData | ReplayResultData>> {
    const result =
        input.commandKind === "TASK"
            ? await apiPost<BackendInvestigationResult>(
                  "/admin/supplier-fulfillment-orders/task-investigations",
                  {
                      work_item_id: input.workItemId,
                      expected_task_version: input.expectedTaskVersion,
                      expected_subject_version: input.expectedSubjectVersion,
                      action: {
                          type: input.action.type,
                          order_id: input.action.orderId,
                          expected_order_lock_version:
                              input.action.expectedOrderLockVersion,
                          target_supplier_action_id:
                              input.action.targetSupplierActionId,
                          operation_id: input.action.operationId,
                      },
                      idempotency_key: input.idempotencyKey,
                  },
              )
            : await apiPost<BackendInvestigationResult>(
                  "/admin/supplier-fulfillment-orders/investigations",
                  {
                      order_id: input.orderId,
                      expected_lock_version: input.expectedLockVersion,
                      action: input.action,
                      operation_id: input.operationId,
                      target_supplier_action_id: input.targetSupplierActionId,
                      idempotency_key: input.idempotencyKey,
                  },
              )
    const evidence = {
        evidenceId: result.evidence.evidence_id,
        targetSupplierActionId: result.evidence.target_supplier_action_id,
        outcome: result.evidence.outcome,
        outcomeLabel:
            result.evidence.outcome === "VERIFIED_TERMINAL"
                ? "处理结果已核实"
                : result.evidence.outcome === "VERIFIED_NO_RESULT"
                  ? "已核实无结果"
                  : "结果仍未知",
        recordedAt: tsToIso(result.evidence.recorded_at),
        canSafeRetry: result.evidence.can_safe_retry,
        externalOrderNo: result.evidence.external_order_no ?? undefined,
        summary: result.evidence.summary,
        verifiedSupplierActionResultId:
            result.evidence.verified_supplier_action_result_id ?? undefined,
        verifiedResolution: result.evidence.verified_resolution ?? undefined,
    }
    const common = {
        status:
            result.result_status === "SUCCEEDED"
                ? ("succeeded" as const)
                : result.result_status === "UNKNOWN"
                  ? ("unknown" as const)
                  : ("blocked" as const),
        message: result.message,
        reference: result.operation_id,
        operationId: result.operation_id,
        data: {
            evidence,
            lockVersion: result.order.version,
            workItemStatus: result.work_item?.status,
            taskVersion:
                result.work_item?.task_version == null
                    ? undefined
                    : String(result.work_item.task_version),
            allowedActions: result.allowed_actions,
            actionBlockers: result.action_blockers.map((blocker) => ({
                action: blocker.action,
                code: blocker.code,
                message: blocker.message,
                destinationWorkspaceId:
                    blocker.destination_workspace_id ?? undefined,
            })),
        },
    }
    if (input.commandKind === "OBJECT" && input.action === "REPLAY") {
        return {
            ...common,
            data: {
                ...common.data,
                externalOrderNo: result.order.external_order_no ?? undefined,
                fulfillmentStatus: asFulfillment(
                    result.order.fulfillment_status,
                ),
            },
        }
    }
    if (input.commandKind === "TASK" && input.action.type === "REPLAY") {
        return {
            ...common,
            data: {
                ...common.data,
                externalOrderNo: result.order.external_order_no ?? undefined,
                fulfillmentStatus: asFulfillment(
                    result.order.fulfillment_status,
                ),
            },
        }
    }
    return common
}

export async function completeSupplierOrderTask(
    input: CompleteSupplierOrderTaskInput,
): Promise<FormalActionResponse<CompleteSupplierOrderTaskResult>> {
    const result = await apiPost<BackendTaskCompletionResult>(
        "/admin/supplier-fulfillment-orders/task-completions",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                type: input.decision.type,
                order_id: input.decision.orderId,
                expected_order_lock_version:
                    input.decision.expectedOrderLockVersion,
                verified_supplier_action_result_id:
                    input.decision.verifiedSupplierActionResultId,
                resolution: input.decision.resolution,
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        message: "已根据可验证结果完成任务。",
        reference: result.operation_id,
        operationId: result.operation_id,
        data: {
            operationId: result.operation_id,
            workItemId: result.work_item_id,
            workItemStatus: result.work_item_status,
            taskVersion: String(result.task_version),
            lockVersion: result.order_lock_version,
            resolution: result.resolution,
        },
    }
}

export async function submitAfterSalesAction(
    input: AfterSalesActionInput,
): Promise<FormalActionResponse<AfterSalesActionResult>> {
    const path =
        input.action === "CANCEL"
            ? `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/cancel`
            : `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/refund`

    const result = await apiPost<BackendSubmitResult>(path, {
        expected_lock_version: input.expectedLockVersion,
        operation_id: input.operationId,
        idempotency_key: input.idempotencyKey,
        after_sales_request_id: input.afterSalesRequestId,
        lines: [],
        reason_code: input.reasonCode,
        comment: input.comment,
    })

    const order = result.order
    return {
        status: "succeeded",
        message:
            input.action === "CANCEL"
                ? "取消动作已提交供应商"
                : "退款动作已提交供应商",
        reference: result.action?.id,
        operationId: input.operationId,
        data: {
            lockVersion: order.version,
            cancelStatus: asCancel(order.cancel_status),
            refundStatus: asRefund(order.refund_status),
            actionRecordId: result.action?.id ?? input.operationId,
            note: "动作已登记",
        },
    }
}

/**
 * 地址揭示：后端详情不返回明文地址（仅加密快照），无 reveal 端点。
 */
export async function revealSupplierOrderAddress(
    input: RevealAddressInput,
): Promise<FormalActionResponse<RevealAddressResult>> {
    void input
    return {
        status: "blocked",
        message: "地址揭示端点尚未交付；详情仅提供脱敏摘要。",
    }
}

export async function clearAddressReveal(orderId: string): Promise<void> {
    void orderId
    // no server session to clear
}

/**
 * 协同说明：后端无 NOTE 端点 → blocked。
 */
export async function addCollaborationNote(
    input: NoteInput,
): Promise<FormalActionResponse<{ lockVersion: number }>> {
    void input
    return {
        status: "blocked",
        message: "协同说明写入端点尚未交付。",
    }
}

export async function createSupplierOrderExportJob(
    command: ExportCommand,
): Promise<ExportJobResult> {
    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: `EXP-W26-${command.requestId.slice(-12)}`,
        job_type: "export",
        domain_job_type: "supplier_fulfillment_order_export",
        selection_snapshot_id: command.selectionSnapshotId || null,
        request_id: command.requestId,
        total_count: Math.max(1, command.rowCount || 1),
        items: [
            {
                object_type: "supplier_fulfillment_order",
                object_id: command.selectionSnapshotId || command.requestId,
            },
        ],
    })

    return {
        jobId: job.id,
        requestId: command.requestId,
        rowCount: command.rowCount,
        permissionVersion: PERMISSION_VERSION,
        fieldSetId: command.fieldSetId,
        maskDisclaimer:
            "导出使用系统筛选快照与字段权限打码：收货地址、手机号不会以明文写入文件。",
        expiresAt: job.result_expires_at
            ? tsToIso(job.result_expires_at)
            : new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(),
        downloadLabel: `供应商订单_${job.job_no ?? job.id}.csv`,
        status: job.status === "completed" ? "succeeded" : "queued",
    }
}
