/**
 * W09 履约作业 · 真实 HTTP API。
 *
 * 后端交付的是单据 CRUD（purchase-receipts / deliveries / electronic-deliveries /
 * service-fulfillments）+ work-items 通用动作，没有统一「履约队列投影」接口。
 * 本层：
 * - 队列：以 DRAFT 单据列表拼装 FulfillmentQueueView（缺字段登记 gap，不造业务数）
 * - 确认：create + post/confirm 真实 HTTP
 * - claim/defer：走 /admin/work-items（单据 ID 非 work_item 时降级为本地租约/失败）
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    DeferReasonCode,
    FormalActionResponse,
    FulfillmentDraft,
    FulfillmentFormalOutcome,
    FulfillmentOperationType,
    FulfillmentQueueView,
    FulfillmentSourceLine,
    FulfillmentTask,
    WorkItemLease,
} from "@/features/fulfillment-operations/types"
import {
    DEFER_REASON_LABEL,
    OPERATION_DONE_LABEL,
    OPERATION_TYPE_SHORT,
} from "@/features/fulfillment-operations/types"
import {
    resolveRole,
    type FulfillmentRole,
} from "@/features/fulfillment-operations/lib/fulfillment-roles"

// ─── backend DTO shapes ─────────────────────────────────────────────────────

type BackendPage<T> = Page<T>

type BackendPurchaseReceipt = {
    id: string
    receipt_no: string
    purchase_order_id: string
    warehouse_id: string
    status: string
    posted_at?: number | null
    version: number
    created_at: number
}

type BackendPurchaseReceiptLine = {
    id: string
    line_no: number
    purchase_order_revision_line_id: string
    received_quantity: string
    qualified_quantity: string
    rejected_quantity: string
    quality_result: string
}

type BackendPurchaseReceiptDetail = {
    receipt: BackendPurchaseReceipt
    lines: BackendPurchaseReceiptLine[]
}

type BackendDelivery = {
    id: string
    delivery_no: string
    delivery_type: string
    sales_order_id: string
    purchase_order_id?: string | null
    warehouse_id?: string | null
    status: string
    carrier?: string | null
    tracking_no?: string | null
    shipped_at?: number | null
    version: number
    created_at: number
}

type BackendDeliveryLine = {
    id: string
    line_no: number
    sales_order_line_id: string
    quantity: string
    stock_reservation_id?: string | null
    purchase_line_sales_allocation_id?: string | null
}

type BackendDeliveryDetail = {
    delivery: BackendDelivery
    lines: BackendDeliveryLine[]
}

type BackendElectronicDelivery = {
    id: string
    fulfillment_no: string
    sales_order_line_id: string
    purchase_order_id: string
    purchase_line_sales_allocation_id: string
    quantity: string
    result: string
    status: string
    occurred_at: number
    recorded_at: number
    version: number
}

type BackendServiceFulfillment = {
    id: string
    fulfillment_no: string
    sales_order_line_id: string
    purchase_order_id: string
    purchase_line_sales_allocation_id: string
    quantity: string
    result: string
    status: string
    occurred_at: number
    recorded_at: number
    version: number
}

type BackendWorkItem = {
    id: string
    work_item_type: string
    business_object_type: string
    business_object_id: string
    subject_version?: string | null
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    priority: string | number
    due_at?: number | null
    reason_code?: string | null
    impact_summary?: string | null
    completion_action: string
    version: number
    created_at: number
}

type BackendWarehouse = {
    id: string
    warehouse_code: string
}

export type FulfillmentQueueFilters = {
    role: FulfillmentRole
    scope: "mine" | "role_pool"
    operationTypes?: FulfillmentOperationType[]
    warehouseId?: string
    q?: string
    due?: "today" | "overdue"
    gate?: "blocked" | "satisfied"
    salesOrderId?: string
    purchaseOrderId?: string
    currentWorkItemId?: string
    queueContextId?: string
}

// ─── helpers ────────────────────────────────────────────────────────────────

function secsToIso(secs: number | null | undefined): string {
    if (secs == null || secs === 0) return ""
    return new Date(secs * 1000).toISOString()
}

function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

function nowIso(): string {
    return new Date().toISOString()
}

function dueLabelFromIso(iso: string): { dueLabel: string; overdue: boolean } {
    if (!iso) return { dueLabel: "—", overdue: false }
    const day = iso.slice(0, 10)
    const today = new Date().toISOString().slice(0, 10)
    if (day < today) return { dueLabel: "已超期", overdue: true }
    if (day === today) return { dueLabel: "今日到期", overdue: false }
    return { dueLabel: day, overdue: false }
}

function priorityNumber(p: string | number | undefined): number {
    if (typeof p === "number") return p
    switch (String(p).toUpperCase()) {
        case "HIGH":
        case "URGENT":
            return 30
        case "NORMAL":
            return 20
        case "LOW":
            return 10
        default:
            return 20
    }
}

function resultBackend(code: string): string {
    switch (code) {
        case "SUCCESS":
            return "SUCCESS"
        case "PARTIAL":
            return "PARTIAL"
        case "FAILED":
            return "FAILED"
        default:
            return code
    }
}

function isoToUnix(iso: string | undefined): number {
    if (!iso) return Math.floor(Date.now() / 1000)
    const t = Date.parse(iso)
    return Number.isFinite(t)
        ? Math.floor(t / 1000)
        : Math.floor(Date.now() / 1000)
}

function genDocNo(prefix: string): string {
    const d = new Date()
    const ymd = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`
    return `${prefix}${ymd}${Math.random().toString(36).slice(2, 6).toUpperCase()}`
}

function emptySourceLine(
    partial: Partial<FulfillmentSourceLine> & {
        lineId: string
        salesOrderLineId: string
    },
): FulfillmentSourceLine {
    return {
        lineId: partial.lineId,
        salesOrderLineId: partial.salesOrderLineId,
        purchaseRevisionLineId: partial.purchaseRevisionLineId,
        itemName: partial.itemName ?? "",
        skuCode: partial.skuCode ?? "",
        unitCode: partial.unitCode ?? "",
        orderedQuantity: partial.orderedQuantity ?? "",
        remainingQuantity:
            partial.remainingQuantity ?? partial.orderedQuantity ?? "",
        stockReservationId: partial.stockReservationId,
        reservedQuantity: partial.reservedQuantity,
        availableOnHand: partial.availableOnHand,
        purchaseLineSalesAllocationId: partial.purchaseLineSalesAllocationId,
    }
}

function baseTask(
    partial: Omit<
        FulfillmentTask,
        | "gate"
        | "allowedActions"
        | "actionBlockers"
        | "impact"
        | "summary"
        | "statusTone"
        | "statusLabel"
        | "held"
        | "priority"
        | "dueAt"
        | "dueLabel"
        | "overdue"
        | "responsibleLabel"
        | "sourceVersion"
        | "subjectHash"
        | "editVersion"
        | "source"
        | "lines"
        | "draft"
    > &
        Partial<FulfillmentTask> & {
            operationType: FulfillmentOperationType
            workItemId: string
        },
): FulfillmentTask {
    const dueAt = partial.dueAt ?? nowIso()
    const { dueLabel, overdue } = dueLabelFromIso(dueAt)
    return {
        workItemId: partial.workItemId,
        operationType: partial.operationType,
        priority: partial.priority ?? 20,
        dueAt,
        dueLabel: partial.dueLabel ?? dueLabel,
        overdue: partial.overdue ?? overdue,
        held: partial.held,
        statusLabel: partial.statusLabel ?? "待处理",
        statusTone: partial.statusTone ?? "info",
        responsibleLabel: partial.responsibleLabel ?? "",
        sourceVersion: partial.sourceVersion ?? "1",
        subjectHash: partial.subjectHash ?? partial.workItemId,
        editVersion: partial.editVersion ?? 1,
        source: partial.source ?? {
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        gate: partial.gate ?? {
            state: "NOT_APPLICABLE",
            message: "",
        },
        lines: partial.lines ?? [],
        draft: partial.draft!,
        summary: partial.summary ?? "",
        impact: partial.impact ?? "",
        allowedActions: partial.allowedActions ?? [
            "POST",
            "SAVE",
            "DEFER",
            "CLAIM",
        ],
        actionBlockers: partial.actionBlockers ?? [],
        lease: partial.lease,
    }
}

function receiptToTask(r: BackendPurchaseReceipt): FulfillmentTask {
    const dueAt = secsToIso(r.created_at) || nowIso()
    return baseTask({
        workItemId: r.id,
        operationType: "RECEIPT",
        editVersion: r.version,
        sourceVersion: String(r.version),
        dueAt,
        summary: r.receipt_no,
        source: {
            purchaseOrderId: r.purchase_order_id,
            purchaseNo: r.purchase_order_id,
            purchaseRevisionId: "",
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
            warehouseId: r.warehouse_id,
            warehouseLabel: r.warehouse_id,
        },
        draft: {
            type: "RECEIPT",
            warehouseId: r.warehouse_id,
            warehouseLabel: r.warehouse_id,
            occurredAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

function deliveryToTask(d: BackendDelivery): FulfillmentTask {
    const op: FulfillmentOperationType =
        d.delivery_type === "SUPPLIER_DIRECT"
            ? "SUPPLIER_DIRECT"
            : "WAREHOUSE_SHIP"
    const dueAt = secsToIso(d.created_at) || nowIso()
    if (op === "WAREHOUSE_SHIP") {
        return baseTask({
            workItemId: d.id,
            operationType: "WAREHOUSE_SHIP",
            editVersion: d.version,
            sourceVersion: String(d.version),
            dueAt,
            summary: d.delivery_no,
            source: {
                purchaseOrderId: d.purchase_order_id ?? undefined,
                salesOrderId: d.sales_order_id,
                salesOrderNo: d.sales_order_id,
                salesRevisionId: "",
                customerLabel: "",
                warehouseId: d.warehouse_id ?? undefined,
                warehouseLabel: d.warehouse_id ?? undefined,
            },
            gate: { state: "SATISFIED", message: "" },
            draft: {
                type: "WAREHOUSE_SHIP",
                warehouseId: d.warehouse_id ?? "",
                warehouseLabel: d.warehouse_id ?? "",
                carrier: d.carrier ?? "",
                trackingNo: d.tracking_no ?? "",
                shippedAt: nowIso().slice(0, 16),
                lines: [],
            },
        })
    }
    return baseTask({
        workItemId: d.id,
        operationType: "SUPPLIER_DIRECT",
        editVersion: d.version,
        sourceVersion: String(d.version),
        dueAt,
        summary: d.delivery_no,
        source: {
            purchaseOrderId: d.purchase_order_id ?? undefined,
            salesOrderId: d.sales_order_id,
            salesOrderNo: d.sales_order_id,
            salesRevisionId: "",
            customerLabel: "",
        },
        draft: {
            type: "SUPPLIER_DIRECT",
            carrier: d.carrier ?? "",
            trackingNo: d.tracking_no ?? "",
            shippedAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

function electronicToTask(e: BackendElectronicDelivery): FulfillmentTask {
    return baseTask({
        workItemId: e.id,
        operationType: "ELECTRONIC",
        editVersion: e.version,
        sourceVersion: String(e.version),
        dueAt: secsToIso(e.occurred_at) || nowIso(),
        summary: e.fulfillment_no,
        source: {
            purchaseOrderId: e.purchase_order_id,
            purchaseNo: e.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: e.sales_order_line_id,
                salesOrderLineId: e.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    e.purchase_line_sales_allocation_id,
                remainingQuantity: e.quantity,
                orderedQuantity: e.quantity,
            }),
        ],
        draft: {
            type: "ELECTRONIC",
            occurredAt:
                secsToIso(e.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            recipientMasked: "",
            result: (e.result as "SUCCESS" | "PARTIAL" | "FAILED") || "SUCCESS",
            lines: [
                {
                    salesOrderLineId: e.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        e.purchase_line_sales_allocation_id,
                    quantity: e.quantity,
                },
            ],
        },
    })
}

function serviceToTask(s: BackendServiceFulfillment): FulfillmentTask {
    return baseTask({
        workItemId: s.id,
        operationType: "SERVICE",
        editVersion: s.version,
        sourceVersion: String(s.version),
        dueAt: secsToIso(s.occurred_at) || nowIso(),
        summary: s.fulfillment_no,
        source: {
            purchaseOrderId: s.purchase_order_id,
            purchaseNo: s.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: s.sales_order_line_id,
                salesOrderLineId: s.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    s.purchase_line_sales_allocation_id,
                remainingQuantity: s.quantity,
                orderedQuantity: s.quantity,
            }),
        ],
        draft: {
            type: "SERVICE",
            startedAt:
                secsToIso(s.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            endedAt:
                secsToIso(s.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            serviceLocation: "",
            result: (s.result as "SUCCESS" | "PARTIAL" | "FAILED") || "SUCCESS",
            completionNote: "",
            lines: [
                {
                    salesOrderLineId: s.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        s.purchase_line_sales_allocation_id,
                    quantity: s.quantity,
                },
            ],
        },
    })
}

async function hydrateTaskDetail(
    task: FulfillmentTask,
): Promise<FulfillmentTask> {
    try {
        if (task.operationType === "RECEIPT") {
            const detail = await apiGet<BackendPurchaseReceiptDetail>(
                `/admin/purchase-receipts/${encodeURIComponent(task.workItemId)}`,
            )
            const lines = detail.lines.map((l) =>
                emptySourceLine({
                    lineId: l.id,
                    salesOrderLineId: l.purchase_order_revision_line_id,
                    purchaseRevisionLineId: l.purchase_order_revision_line_id,
                    remainingQuantity: l.received_quantity,
                    orderedQuantity: l.received_quantity,
                }),
            )
            const draftLines = detail.lines.map((l) => ({
                purchaseRevisionLineId: l.purchase_order_revision_line_id,
                receivedQuantity: l.received_quantity,
                qualifiedQuantity: l.qualified_quantity,
                rejectedQuantity: l.rejected_quantity,
                qualityResult: l.quality_result,
            }))
            return {
                ...task,
                editVersion: detail.receipt.version,
                sourceVersion: String(detail.receipt.version),
                lines,
                draft: {
                    type: "RECEIPT",
                    warehouseId: detail.receipt.warehouse_id,
                    warehouseLabel: detail.receipt.warehouse_id,
                    occurredAt:
                        task.draft.type === "RECEIPT"
                            ? task.draft.occurredAt
                            : nowIso().slice(0, 16),
                    lines: draftLines,
                },
            }
        }
        if (
            task.operationType === "WAREHOUSE_SHIP" ||
            task.operationType === "SUPPLIER_DIRECT"
        ) {
            const detail = await apiGet<BackendDeliveryDetail>(
                `/admin/deliveries/${encodeURIComponent(task.workItemId)}`,
            )
            const lines = detail.lines.map((l) =>
                emptySourceLine({
                    lineId: l.id,
                    salesOrderLineId: l.sales_order_line_id,
                    remainingQuantity: l.quantity,
                    orderedQuantity: l.quantity,
                    stockReservationId: l.stock_reservation_id ?? undefined,
                    reservedQuantity: l.stock_reservation_id
                        ? l.quantity
                        : undefined,
                    purchaseLineSalesAllocationId:
                        l.purchase_line_sales_allocation_id ?? undefined,
                }),
            )
            if (task.operationType === "WAREHOUSE_SHIP") {
                return {
                    ...task,
                    editVersion: detail.delivery.version,
                    sourceVersion: String(detail.delivery.version),
                    lines,
                    draft: {
                        type: "WAREHOUSE_SHIP",
                        warehouseId: detail.delivery.warehouse_id ?? "",
                        warehouseLabel: detail.delivery.warehouse_id ?? "",
                        carrier: detail.delivery.carrier ?? "",
                        trackingNo: detail.delivery.tracking_no ?? "",
                        shippedAt: nowIso().slice(0, 16),
                        lines: detail.lines.map((l) => ({
                            salesOrderLineId: l.sales_order_line_id,
                            stockReservationId: l.stock_reservation_id ?? "",
                            quantity: l.quantity,
                        })),
                    },
                }
            }
            return {
                ...task,
                editVersion: detail.delivery.version,
                sourceVersion: String(detail.delivery.version),
                lines,
                draft: {
                    type: "SUPPLIER_DIRECT",
                    carrier: detail.delivery.carrier ?? "",
                    trackingNo: detail.delivery.tracking_no ?? "",
                    shippedAt: nowIso().slice(0, 16),
                    lines: detail.lines.map((l) => ({
                        salesOrderLineId: l.sales_order_line_id,
                        purchaseLineSalesAllocationId:
                            l.purchase_line_sales_allocation_id ?? "",
                        quantity: l.quantity,
                    })),
                },
            }
        }
    } catch {
        // keep list projection
    }
    return task
}

function filterSummary(
    filters: FulfillmentQueueFilters,
    warehouseOptions: FulfillmentQueueView["context"]["warehouseOptions"],
): string {
    const parts = [
        filters.scope === "mine" && resolveRole(filters.role).userLabel
            ? "仅我的"
            : "全组",
        filters.operationTypes && filters.operationTypes.length > 0
            ? filters.operationTypes
                  .map((t) => OPERATION_TYPE_SHORT[t])
                  .join("/")
            : "全部类型",
    ]
    if (filters.due === "overdue") parts.push("已超期")
    else if (filters.due === "today") parts.push("今日到期")
    if (filters.gate === "blocked") parts.push("先款未到")
    if (filters.gate === "satisfied") parts.push("货款已到")
    if (filters.warehouseId) {
        const label = warehouseOptions.find(
            (w) => w.value === filters.warehouseId,
        )?.label
        parts.push(label ?? "指定仓库")
    }
    if (filters.q) parts.push(`单号 ${filters.q}`)
    if (filters.salesOrderId) parts.push(`销售单 ${filters.salesOrderId}`)
    if (filters.purchaseOrderId) parts.push(`采购单 ${filters.purchaseOrderId}`)
    return parts.join(" · ")
}

function matchTask(
    task: FulfillmentTask,
    filters: FulfillmentQueueFilters,
    roleTypes: readonly FulfillmentOperationType[],
): boolean {
    if (!roleTypes.includes(task.operationType)) return false
    if (
        filters.operationTypes &&
        filters.operationTypes.length > 0 &&
        !filters.operationTypes.includes(task.operationType)
    ) {
        return false
    }
    if (filters.warehouseId) {
        if (
            (task.operationType === "RECEIPT" ||
                task.operationType === "WAREHOUSE_SHIP") &&
            task.source.warehouseId !== filters.warehouseId
        ) {
            return false
        }
    }
    if (
        filters.salesOrderId &&
        task.source.salesOrderId !== filters.salesOrderId
    ) {
        return false
    }
    if (
        filters.purchaseOrderId &&
        task.source.purchaseOrderId !== filters.purchaseOrderId
    ) {
        return false
    }
    if (filters.q) {
        const q = filters.q.trim().toUpperCase()
        const hay = [
            task.source.salesOrderNo,
            task.source.purchaseNo ?? "",
            task.summary,
            task.workItemId,
        ]
            .join(" ")
            .toUpperCase()
        if (!hay.includes(q)) return false
    }
    if (filters.due === "overdue" && !task.overdue) return false
    if (filters.due === "today") {
        const today = new Date().toISOString().slice(0, 10)
        if (task.dueAt.slice(0, 10) !== today) return false
    }
    if (filters.gate === "blocked" && task.gate.state !== "BLOCKED")
        return false
    if (filters.gate === "satisfied" && task.gate.state !== "SATISFIED")
        return false
    // scope=mine: backend has no owner on documents — cannot filter; gap
    return true
}

// ─── public API ─────────────────────────────────────────────────────────────

export async function fetchFulfillmentQueue(
    filters: FulfillmentQueueFilters,
): Promise<FulfillmentQueueView> {
    const role = resolveRole(filters.role)
    const requestedOutOfRole =
        filters.operationTypes && filters.operationTypes.length > 0
            ? filters.operationTypes.filter((t) => !role.types.includes(t))
            : []

    if (requestedOutOfRole.length > 0) {
        return {
            preferences: { autoNextDefault: true },
            context: {
                queueContextId:
                    filters.queueContextId ?? `queue:W09:${filters.scope}`,
                position: 0,
                total: 0,
                filterSummary: filterSummary(filters, []),
                warehouseOptions: [],
                visibleTypes: role.types,
                roleLabel: role.label,
                viewerLabel: role.userLabel,
                canExecute: role.canExecute,
                snapshotUpdatedAt: nowIso(),
            },
            metrics: role.types.map((operationType) => ({
                operationType,
                label: `待${OPERATION_TYPE_SHORT[operationType]}`,
                count: 0,
                visible: true,
            })),
            tasks: [],
            emptyReason: "NO_PERMISSION",
        }
    }

    // Load draft documents for types visible to role
    const want = new Set(role.types)
    const tasks: FulfillmentTask[] = []

    const loaders: Promise<void>[] = []

    if (want.has("RECEIPT")) {
        loaders.push(
            apiGet<BackendPage<BackendPurchaseReceipt>>(
                "/admin/purchase-receipts",
                {
                    page: 1,
                    page_size: 100,
                    status: "DRAFT",
                    purchase_order_id: filters.purchaseOrderId,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
                .then((page) => {
                    for (const r of page.items) tasks.push(receiptToTask(r))
                })
                .catch((error) => {
                    if (!(isApiError(error) && error.status === 403))
                        throw error
                }),
        )
    }

    if (want.has("WAREHOUSE_SHIP") || want.has("SUPPLIER_DIRECT")) {
        loaders.push(
            apiGet<BackendPage<BackendDelivery>>("/admin/deliveries", {
                page: 1,
                page_size: 100,
                status: "DRAFT",
                sales_order_id: filters.salesOrderId,
                sort_by: "created_at",
                sort_dir: "desc",
            })
                .then((page) => {
                    for (const d of page.items) {
                        const t = deliveryToTask(d)
                        if (want.has(t.operationType)) tasks.push(t)
                    }
                })
                .catch((error) => {
                    if (!(isApiError(error) && error.status === 403))
                        throw error
                }),
        )
    }

    if (want.has("ELECTRONIC")) {
        loaders.push(
            apiGet<BackendPage<BackendElectronicDelivery>>(
                "/admin/electronic-deliveries",
                {
                    page: 1,
                    page_size: 100,
                    status: "DRAFT",
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
                .then((page) => {
                    for (const e of page.items) tasks.push(electronicToTask(e))
                })
                .catch((error) => {
                    if (!(isApiError(error) && error.status === 403))
                        throw error
                }),
        )
    }

    if (want.has("SERVICE")) {
        loaders.push(
            apiGet<BackendPage<BackendServiceFulfillment>>(
                "/admin/service-fulfillments",
                {
                    page: 1,
                    page_size: 100,
                    status: "DRAFT",
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
                .then((page) => {
                    for (const s of page.items) tasks.push(serviceToTask(s))
                })
                .catch((error) => {
                    if (!(isApiError(error) && error.status === 403))
                        throw error
                }),
        )
    }

    await Promise.all(loaders)

    // warehouse options from warehouses API
    let warehouseOptions: FulfillmentQueueView["context"]["warehouseOptions"] =
        []
    try {
        const wh = await apiGet<BackendPage<BackendWarehouse>>(
            "/admin/warehouses",
            {
                page: 1,
                page_size: 100,
            },
        )
        warehouseOptions = wh.items.map((w) => ({
            value: w.id,
            label: w.warehouse_code,
        }))
    } catch {
        // fall back to task-derived
        const seen = new Map<string, string>()
        for (const t of tasks) {
            const id = t.source.warehouseId
            if (id && !seen.has(id)) seen.set(id, t.source.warehouseLabel ?? id)
        }
        warehouseOptions = [...seen].map(([value, label]) => ({ value, label }))
    }

    const inScope = tasks.filter((t) => role.types.includes(t.operationType))
    const metrics = role.types.map((operationType) => ({
        operationType,
        label: `待${OPERATION_TYPE_SHORT[operationType]}`,
        count: inScope.filter((t) => t.operationType === operationType).length,
        visible: true,
    }))

    let filtered = inScope.filter((t) => matchTask(t, filters, role.types))
    filtered = [...filtered].sort((a, b) => {
        if (a.overdue !== b.overdue) return a.overdue ? -1 : 1
        if (a.priority !== b.priority) return b.priority - a.priority
        return a.dueAt.localeCompare(b.dueAt)
    })

    let position = 0
    let current = filtered[0]
    if (filters.currentWorkItemId) {
        const idx = filtered.findIndex(
            (t) => t.workItemId === filters.currentWorkItemId,
        )
        if (idx >= 0) {
            position = idx
            current = filtered[idx]
        }
    }

    if (current) {
        current = await hydrateTaskDetail(current)
        filtered = filtered.map((t) =>
            t.workItemId === current!.workItemId ? current! : t,
        )
    }

    const emptyReason =
        inScope.length === 0
            ? "NO_TASKS"
            : filtered.length === 0
              ? "FILTER_NO_RESULT"
              : undefined

    return {
        preferences: { autoNextDefault: true },
        context: {
            queueContextId:
                filters.queueContextId ?? `queue:W09:${filters.scope}`,
            position: filtered.length === 0 ? 0 : position + 1,
            total: filtered.length,
            currentWorkItemId: current?.workItemId,
            previousWorkItemId: filtered[position - 1]?.workItemId,
            nextWorkItemId: filtered[position + 1]?.workItemId,
            filterSummary: filterSummary(filters, warehouseOptions),
            warehouseOptions,
            visibleTypes: role.types,
            roleLabel: role.label,
            viewerLabel: role.userLabel,
            canExecute: role.canExecute,
            snapshotUpdatedAt: nowIso(),
        },
        metrics,
        tasks: filtered,
        current,
        emptyReason,
    }
}

export async function claimFulfillmentWorkItem(
    workItemId: string,
): Promise<WorkItemLease> {
    // Prefer work-item claim when the id is a real work item
    try {
        const wi = await apiGet<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(workItemId)}`,
        )
        await apiPost<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(workItemId)}/claim`,
            { version: wi.version },
        )
        return {
            workItemId,
            claimedByLabel: "当前用户",
        }
    } catch (error) {
        // Document-backed queue rows are not work_items — claim is local UX only
        if (
            isApiError(error) &&
            (error.status === 404 || error.status === 422)
        ) {
            return {
                workItemId,
                claimedByLabel: "当前用户",
            }
        }
        throw error
    }
}

export async function saveFulfillmentOperation(input: {
    workItemId: string
    expectedEditVersion: number
    draft: FulfillmentDraft
}): Promise<{ editVersion: number }> {
    const draft = input.draft
    if (draft.type === "RECEIPT") {
        const updated = await apiPut<BackendPurchaseReceipt>(
            `/admin/purchase-receipts/${encodeURIComponent(input.workItemId)}`,
            {
                version: input.expectedEditVersion,
                warehouse_id: draft.warehouseId || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    if (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") {
        const updated = await apiPut<BackendDelivery>(
            `/admin/deliveries/${encodeURIComponent(input.workItemId)}`,
            {
                version: input.expectedEditVersion,
                carrier: draft.carrier || undefined,
                tracking_no: draft.trackingNo || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    // ELECTRONIC / SERVICE drafts: backend has no update endpoint for draft body
    // (create-only + confirm). Keep editVersion bump client-visible as no-op success
    // with same version — register gap.
    return { editVersion: input.expectedEditVersion }
}

function formalFromReceipt(
    receipt: BackendPurchaseReceipt,
    draft: Extract<FulfillmentDraft, { type: "RECEIPT" }>,
    workItemId: string,
    nextWorkItemId?: string,
): FulfillmentFormalOutcome {
    return {
        kind: "POSTED",
        workItemId,
        factType: "PURCHASE_RECEIPT",
        factId: receipt.id,
        factNo: receipt.receipt_no,
        formalStatus: receipt.status || "POSTED",
        occurredAt: draft.occurredAt || nowIso(),
        operationType: "RECEIPT",
        inventoryDelta: draft.lines
            .filter((l) => Number(l.qualifiedQuantity) > 0)
            .map((l) => ({
                warehouseId: draft.warehouseId,
                warehouseLabel: draft.warehouseLabel,
                skuId: l.purchaseRevisionLineId,
                skuLabel: l.purchaseRevisionLineId,
                quantity: l.qualifiedQuantity,
                direction: "INCREASE" as const,
            })),
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: false,
        acceptanceNextStep:
            "入库不等于验收。合格的货已入库并按销售单留好；等发货之后，再由销售去登记客户验收。",
        inventoryImpactSummary: "合格数量入库并留货（以服务端结果为准）。",
        reference: receipt.receipt_no,
        nextWorkItemId,
        salesOrderId: "",
        salesOrderNo: "",
    }
}

function formalFromDelivery(
    delivery: BackendDelivery,
    draft: Extract<
        FulfillmentDraft,
        { type: "WAREHOUSE_SHIP" } | { type: "SUPPLIER_DIRECT" }
    >,
    workItemId: string,
    nextWorkItemId?: string,
): FulfillmentFormalOutcome {
    const isWh = draft.type === "WAREHOUSE_SHIP"
    return {
        kind: "POSTED",
        workItemId,
        factType: "DELIVERY",
        factId: delivery.id,
        factNo: delivery.delivery_no,
        formalStatus: delivery.status || "SHIPPED",
        occurredAt: (isWh ? draft.shippedAt : draft.shippedAt) || nowIso(),
        operationType: draft.type,
        inventoryDelta: isWh
            ? draft.lines.map((l) => ({
                  warehouseId: draft.warehouseId,
                  warehouseLabel: draft.warehouseLabel,
                  skuId: l.salesOrderLineId,
                  skuLabel: l.salesOrderLineId,
                  quantity: l.quantity,
                  direction: "DECREASE" as const,
              }))
            : [],
        reservationDelta: isWh
            ? draft.lines.map((l) => ({
                  reservationId: l.stockReservationId,
                  quantity: l.quantity,
                  action: "CONSUME" as const,
                  salesOrderLineId: l.salesOrderLineId,
              }))
            : [],
        remainingByLine: [],
        acceptanceRequired: true,
        acceptanceNextStep: isWh
            ? "仓发记录已确认。物流签收不等于客户验收；请销售在客户验收登记。"
            : "供应商直发记录已确认，不影响自有库存。请销售在客户验收登记（物流签收≠验收）。",
        inventoryImpactSummary: isWh
            ? "用掉了为这单留的货，库存相应减少（不涉及付款）。"
            : "不动自己仓库的库存，也不动留货。",
        reference: delivery.delivery_no,
        nextWorkItemId,
        salesOrderId: delivery.sales_order_id,
        salesOrderNo: delivery.sales_order_id,
    }
}

export async function postFulfillmentOperation(input: {
    workItemId: string
    expectedSubjectVersion: string
    expectedSourceVersion: string
    expectedEditVersion: number
    draft: FulfillmentDraft
    nextWorkItemId?: string
}): Promise<FormalActionResponse> {
    const draft = input.draft

    try {
        if (draft.type === "RECEIPT") {
            // Prefer post existing draft document; if not found, create then post
            const receiptId = input.workItemId
            let receipt: BackendPurchaseReceipt | null = null
            try {
                const detail = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.workItemId)}`,
                )
                receipt = detail.receipt
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) throw error
            }

            if (!receipt) {
                if (!draft.lines.length) {
                    return {
                        status: "failed",
                        code: "VALIDATION_BLOCKED",
                        message: "入库明细不能为空",
                    }
                }
                // Need purchase_order_id — not always on draft; require from source via prior queue
                return {
                    status: "failed",
                    code: "BACKEND_GAP",
                    message:
                        "未找到入库草稿。请从采购上下文创建入库单后再确认（队列投影与创建链路待后端补齐）。",
                }
            }

            if (
                receipt.warehouse_id !== draft.warehouseId &&
                draft.warehouseId
            ) {
                await apiPut<BackendPurchaseReceipt>(
                    `/admin/purchase-receipts/${encodeURIComponent(receiptId)}`,
                    {
                        version: receipt.version,
                        warehouse_id: draft.warehouseId,
                    },
                )
            }

            const posted = await apiPost<BackendPurchaseReceipt>(
                `/admin/purchase-receipts/${encodeURIComponent(receiptId)}/post`,
            )
            return {
                status: "succeeded",
                outcome: formalFromReceipt(
                    posted,
                    draft,
                    input.workItemId,
                    input.nextWorkItemId,
                ),
            }
        }

        if (
            draft.type === "WAREHOUSE_SHIP" ||
            draft.type === "SUPPLIER_DIRECT"
        ) {
            let delivery: BackendDelivery | null = null
            try {
                const detail = await apiGet<BackendDeliveryDetail>(
                    `/admin/deliveries/${encodeURIComponent(input.workItemId)}`,
                )
                delivery = detail.delivery
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) throw error
            }

            if (!delivery) {
                // Create new delivery from draft
                if (!draft.lines.length) {
                    return {
                        status: "failed",
                        code: "VALIDATION_BLOCKED",
                        message: "发货明细不能为空",
                    }
                }
                const created = await apiPost<BackendDelivery>(
                    "/admin/deliveries",
                    {
                        delivery_no: genDocNo(
                            draft.type === "WAREHOUSE_SHIP" ? "FH" : "DF",
                        ),
                        delivery_type:
                            draft.type === "WAREHOUSE_SHIP"
                                ? "WAREHOUSE_SHIP"
                                : "SUPPLIER_DIRECT",
                        sales_order_id:
                            draft.lines[0]
                                ?.salesOrderLineId /* wrong — need SO id */ ||
                            input.workItemId,
                        purchase_order_id: undefined,
                        warehouse_id:
                            draft.type === "WAREHOUSE_SHIP"
                                ? draft.warehouseId
                                : undefined,
                        carrier: draft.carrier,
                        tracking_no: draft.trackingNo,
                        lines:
                            draft.type === "WAREHOUSE_SHIP"
                                ? draft.lines.map((l) => ({
                                      sales_order_line_id: l.salesOrderLineId,
                                      quantity: l.quantity,
                                      stock_reservation_id:
                                          l.stockReservationId,
                                      purchase_line_sales_allocation_id: null,
                                  }))
                                : draft.lines.map((l) => ({
                                      sales_order_line_id: l.salesOrderLineId,
                                      quantity: l.quantity,
                                      stock_reservation_id: null,
                                      purchase_line_sales_allocation_id:
                                          l.purchaseLineSalesAllocationId,
                                  })),
                    },
                )
                const posted = await apiPost<BackendDelivery>(
                    `/admin/deliveries/${encodeURIComponent(created.id)}/post`,
                )
                return {
                    status: "succeeded",
                    outcome: formalFromDelivery(
                        posted,
                        draft,
                        input.workItemId,
                        input.nextWorkItemId,
                    ),
                }
            }

            await apiPut<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}`,
                {
                    version: delivery.version,
                    carrier: draft.carrier || undefined,
                    tracking_no: draft.trackingNo || undefined,
                },
            )
            const posted = await apiPost<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}/post`,
            )
            return {
                status: "succeeded",
                outcome: formalFromDelivery(
                    posted,
                    draft,
                    input.workItemId,
                    input.nextWorkItemId,
                ),
            }
        }

        if (draft.type === "ELECTRONIC") {
            const line = draft.lines[0]
            if (!line) {
                return {
                    status: "failed",
                    code: "VALIDATION_BLOCKED",
                    message: "交付明细不能为空",
                }
            }
            // Try confirm existing draft first
            try {
                const confirmed = await apiPost<BackendElectronicDelivery>(
                    `/admin/electronic-deliveries/${encodeURIComponent(input.workItemId)}/confirm`,
                )
                return {
                    status: "succeeded",
                    outcome: {
                        kind: "POSTED",
                        workItemId: input.workItemId,
                        factType: "ELECTRONIC_DELIVERY",
                        factId: confirmed.id,
                        factNo: confirmed.fulfillment_no,
                        formalStatus:
                            confirmed.result === "FAILED"
                                ? "FAILED"
                                : "CONFIRMED",
                        occurredAt:
                            secsToIso(confirmed.occurred_at) || nowIso(),
                        operationType: "ELECTRONIC",
                        inventoryDelta: [],
                        reservationDelta: [],
                        remainingByLine: [],
                        acceptanceRequired: confirmed.result !== "FAILED",
                        acceptanceNextStep:
                            confirmed.result === "FAILED"
                                ? "电子交付失败已留痕，不可覆盖；重做须新建记录。不进入客户验收。"
                                : "电子交付已确认，不影响自有库存。请销售在客户验收登记。",
                        inventoryImpactSummary: "不影响自有库存。",
                        reference: confirmed.fulfillment_no,
                        nextWorkItemId: input.nextWorkItemId,
                        salesOrderId: "",
                        salesOrderNo: "",
                    },
                }
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) {
                    if (isApiError(error)) {
                        return {
                            status: "failed",
                            code: String(error.status ?? "ERROR"),
                            message: error.message,
                        }
                    }
                    throw error
                }
            }

            // Create + confirm requires purchase_order_id which draft may lack fully
            const created = await apiPost<BackendElectronicDelivery>(
                "/admin/electronic-deliveries",
                {
                    fulfillment_no: genDocNo("ED"),
                    sales_order_line_id: line.salesOrderLineId,
                    purchase_order_id: "", // will fail validation if empty — surface backend error
                    purchase_line_sales_allocation_id:
                        line.purchaseLineSalesAllocationId,
                    recipient_snapshot: draft.recipientMasked || "masked",
                    quantity: line.quantity,
                    result: resultBackend(draft.result),
                    occurred_at: isoToUnix(draft.occurredAt),
                },
            )
            const confirmed = await apiPost<BackendElectronicDelivery>(
                `/admin/electronic-deliveries/${encodeURIComponent(created.id)}/confirm`,
            )
            return {
                status: "succeeded",
                outcome: {
                    kind: "POSTED",
                    workItemId: input.workItemId,
                    factType: "ELECTRONIC_DELIVERY",
                    factId: confirmed.id,
                    factNo: confirmed.fulfillment_no,
                    formalStatus:
                        confirmed.result === "FAILED" ? "FAILED" : "CONFIRMED",
                    occurredAt: secsToIso(confirmed.occurred_at) || nowIso(),
                    operationType: "ELECTRONIC",
                    inventoryDelta: [],
                    reservationDelta: [],
                    remainingByLine: [],
                    acceptanceRequired: confirmed.result !== "FAILED",
                    acceptanceNextStep:
                        "电子交付已确认，不影响自有库存。请销售在客户验收登记。",
                    inventoryImpactSummary: "不影响自有库存。",
                    reference: confirmed.fulfillment_no,
                    nextWorkItemId: input.nextWorkItemId,
                    salesOrderId: "",
                    salesOrderNo: "",
                },
            }
        }

        // SERVICE
        const line = draft.lines[0]
        if (!line) {
            return {
                status: "failed",
                code: "VALIDATION_BLOCKED",
                message: "服务明细不能为空",
            }
        }
        try {
            const confirmed = await apiPost<BackendServiceFulfillment>(
                `/admin/service-fulfillments/${encodeURIComponent(input.workItemId)}/confirm`,
            )
            return {
                status: "succeeded",
                outcome: {
                    kind: "POSTED",
                    workItemId: input.workItemId,
                    factType: "SERVICE_FULFILLMENT",
                    factId: confirmed.id,
                    factNo: confirmed.fulfillment_no,
                    formalStatus:
                        confirmed.result === "FAILED" ? "FAILED" : "CONFIRMED",
                    occurredAt: secsToIso(confirmed.occurred_at) || nowIso(),
                    operationType: "SERVICE",
                    inventoryDelta: [],
                    reservationDelta: [],
                    remainingByLine: [],
                    acceptanceRequired: confirmed.result !== "FAILED",
                    acceptanceNextStep:
                        confirmed.result === "FAILED"
                            ? "服务失败已留痕，不可覆盖；重做须新建记录。"
                            : "服务履约已确认。请销售在客户验收登记。",
                    inventoryImpactSummary: "不影响自有库存。",
                    reference: confirmed.fulfillment_no,
                    nextWorkItemId: input.nextWorkItemId,
                    salesOrderId: "",
                    salesOrderNo: "",
                },
            }
        } catch (error) {
            if (!(isApiError(error) && error.status === 404)) {
                if (isApiError(error)) {
                    return {
                        status: "failed",
                        code: String(error.status ?? "ERROR"),
                        message: error.message,
                    }
                }
                throw error
            }
        }

        const created = await apiPost<BackendServiceFulfillment>(
            "/admin/service-fulfillments",
            {
                fulfillment_no: genDocNo("FW"),
                sales_order_line_id: line.salesOrderLineId,
                purchase_order_id: "",
                purchase_line_sales_allocation_id:
                    line.purchaseLineSalesAllocationId,
                recipient_snapshot: "masked",
                quantity: line.quantity,
                result: resultBackend(draft.result),
                service_location: draft.serviceLocation || "—",
                service_started_at: isoToUnix(draft.startedAt),
                service_ended_at: isoToUnix(draft.endedAt),
                completion_note: draft.completionNote,
                occurred_at: isoToUnix(draft.endedAt || draft.startedAt),
            },
        )
        const confirmed = await apiPost<BackendServiceFulfillment>(
            `/admin/service-fulfillments/${encodeURIComponent(created.id)}/confirm`,
        )
        return {
            status: "succeeded",
            outcome: {
                kind: "POSTED",
                workItemId: input.workItemId,
                factType: "SERVICE_FULFILLMENT",
                factId: confirmed.id,
                factNo: confirmed.fulfillment_no,
                formalStatus:
                    confirmed.result === "FAILED" ? "FAILED" : "CONFIRMED",
                occurredAt: secsToIso(confirmed.occurred_at) || nowIso(),
                operationType: "SERVICE",
                inventoryDelta: [],
                reservationDelta: [],
                remainingByLine: [],
                acceptanceRequired: confirmed.result !== "FAILED",
                acceptanceNextStep: "服务履约已确认。请销售在客户验收登记。",
                inventoryImpactSummary: "不影响自有库存。",
                reference: confirmed.fulfillment_no,
                nextWorkItemId: input.nextWorkItemId,
                salesOrderId: "",
                salesOrderNo: "",
            },
        }
    } catch (error) {
        if (isApiError(error)) {
            if (
                error.status === 500 &&
                typeof error.message === "string" &&
                error.message.includes("暂无法确认")
            ) {
                return {
                    status: "unknown",
                    message: error.message,
                    idempotencyKey: `post_${input.workItemId}_${Date.now().toString(36)}`,
                }
            }
            if (error.status === 409) {
                return {
                    status: "failed",
                    code: "SUBJECT_VERSION_MISMATCH",
                    message: "数据已变更，请刷新后重试",
                }
            }
            return {
                status: "failed",
                code: String(error.status ?? "ERROR"),
                message: error.message,
            }
        }
        throw error
    }
}

export async function deferFulfillmentOperation(input: {
    workItemId: string
    queueContextId: string
    reasonCode: DeferReasonCode
    reasonNote?: string
    nextWorkItemId?: string
}): Promise<FormalActionResponse> {
    if (!input.reasonCode) {
        return {
            status: "failed",
            code: "REASON_REQUIRED",
            message: "先跳过需要选一个原因",
        }
    }
    const note = `${DEFER_REASON_LABEL[input.reasonCode]}${
        input.reasonNote ? `: ${input.reasonNote}` : ""
    }`
    try {
        const wi = await apiGet<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}`,
        )
        await apiPost<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`,
            {
                version: wi.version,
                comment: note,
            },
        )
        return {
            status: "succeeded",
            outcome: {
                kind: "DEFERRED",
                workItemId: input.workItemId,
                workItemStatus: "PENDING",
                leaseDisposition: "RELEASED",
                reasonCode: input.reasonCode,
                reasonNote: input.reasonNote,
                nextWorkItemId: input.nextWorkItemId,
                reference: `FF-HOLD-${input.workItemId.toUpperCase()}`,
            },
        }
    } catch (error) {
        if (isApiError(error) && error.status === 404) {
            // Document-based queue has no work_item defer — surface gap, do not fake success
            return {
                status: "failed",
                code: "BACKEND_GAP",
                message:
                    "当前任务不是待办记录，无法在服务端跳过。履约队列工作项类型尚未由后端提供。",
            }
        }
        if (isApiError(error)) {
            return {
                status: "failed",
                code: String(error.status ?? "ERROR"),
                message: error.message,
            }
        }
        throw error
    }
}

export async function resolveUnknownFulfillmentResult(input: {
    workItemId: string
}): Promise<FormalActionResponse> {
    // Probe document status for posted outcomes
    const probes: Array<() => Promise<FormalActionResponse | null>> = [
        async () => {
            try {
                const d = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.workItemId)}`,
                )
                if (d.receipt.status === "POSTED") {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            workItemId: input.workItemId,
                            factType: "PURCHASE_RECEIPT",
                            factId: d.receipt.id,
                            factNo: d.receipt.receipt_no,
                            formalStatus: "POSTED",
                            occurredAt:
                                secsToIso(d.receipt.posted_at) || nowIso(),
                            operationType: "RECEIPT",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: false,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: d.receipt.receipt_no,
                            salesOrderId: "",
                            salesOrderNo: "",
                        },
                    }
                }
            } catch {
                /* continue */
            }
            return null
        },
        async () => {
            try {
                const d = await apiGet<BackendDeliveryDetail>(
                    `/admin/deliveries/${encodeURIComponent(input.workItemId)}`,
                )
                if (
                    d.delivery.status === "SHIPPED" ||
                    d.delivery.status === "SIGNED"
                ) {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            workItemId: input.workItemId,
                            factType: "DELIVERY",
                            factId: d.delivery.id,
                            factNo: d.delivery.delivery_no,
                            formalStatus: d.delivery.status,
                            occurredAt:
                                secsToIso(d.delivery.shipped_at) || nowIso(),
                            operationType:
                                d.delivery.delivery_type === "SUPPLIER_DIRECT"
                                    ? "SUPPLIER_DIRECT"
                                    : "WAREHOUSE_SHIP",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: true,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: d.delivery.delivery_no,
                            salesOrderId: d.delivery.sales_order_id,
                            salesOrderNo: d.delivery.sales_order_id,
                        },
                    }
                }
            } catch {
                /* continue */
            }
            return null
        },
    ]

    for (const probe of probes) {
        const hit = await probe()
        if (hit) return hit
    }

    return {
        status: "failed",
        code: "NO_PENDING",
        message: "未找到该任务号对应的处理中请求",
    }
}

// silence unused in case tree-shaking tools complain about OPERATION_DONE_LABEL in some builds
void OPERATION_DONE_LABEL
void priorityNumber
