/**
 * W09 履约单据处理 · 真实 HTTP API。
 *
 * 当前服务端提供采购入库、发货、电子交付和服务履约单据接口，尚未注册
 * W09 专属正式任务。这里仅投影 DRAFT 单据并直达各领域强类型命令；不得把
 * 单据 ID 冒充 work_item，也不得用客户端责任状态补足缺失的任务合同。
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    FormalActionResponse,
    FulfillmentDraft,
    FulfillmentFormalOutcome,
    FulfillmentOperationType,
    FulfillmentQueueView,
    FulfillmentSourceLine,
    FulfillmentOperation,
    PostFulfillmentOperationCommand,
    ResolveFulfillmentOperationCommand,
    SaveFulfillmentOperationCommand,
} from "@/features/fulfillment-operations/types"
import { OPERATION_TYPE_SHORT } from "@/features/fulfillment-operations/types"
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

type BackendWarehouse = {
    id: string
    warehouse_code: string
}

export type FulfillmentQueueFilters = {
    role: FulfillmentRole
    operationTypes?: FulfillmentOperationType[]
    warehouseId?: string
    q?: string
    due?: "today" | "overdue"
    gate?: "blocked" | "satisfied"
    salesOrderId?: string
    purchaseOrderId?: string
    currentOperationId?: string
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

function baseOperation(
    partial: Omit<
        FulfillmentOperation,
        | "gate"
        | "actionBlockers"
        | "impact"
        | "summary"
        | "statusTone"
        | "statusLabel"
        | "priority"
        | "dueAt"
        | "dueLabel"
        | "overdue"
        | "responsibleLabel"
        | "sourceVersion"
        | "editVersion"
        | "source"
        | "lines"
        | "draft"
    > &
        Partial<FulfillmentOperation> & {
            operationType: FulfillmentOperationType
            operationId: string
        },
): FulfillmentOperation {
    const dueAt = partial.dueAt ?? nowIso()
    const { dueLabel, overdue } = dueLabelFromIso(dueAt)
    return {
        operationId: partial.operationId,
        operationType: partial.operationType,
        priority: partial.priority ?? 20,
        dueAt,
        dueLabel: partial.dueLabel ?? dueLabel,
        overdue: partial.overdue ?? overdue,
        statusLabel: partial.statusLabel ?? "待处理",
        statusTone: partial.statusTone ?? "info",
        responsibleLabel: partial.responsibleLabel ?? "",
        sourceVersion: partial.sourceVersion ?? "1",
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
        actionBlockers: partial.actionBlockers ?? [],
    }
}

function receiptToOperation(r: BackendPurchaseReceipt): FulfillmentOperation {
    const dueAt = secsToIso(r.created_at) || nowIso()
    return baseOperation({
        operationId: r.id,
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

function deliveryToOperation(d: BackendDelivery): FulfillmentOperation {
    const op: FulfillmentOperationType =
        d.delivery_type === "SUPPLIER_DIRECT"
            ? "SUPPLIER_DIRECT"
            : "WAREHOUSE_SHIP"
    const dueAt = secsToIso(d.created_at) || nowIso()
    if (op === "WAREHOUSE_SHIP") {
        return baseOperation({
            operationId: d.id,
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
    return baseOperation({
        operationId: d.id,
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

function electronicToOperation(
    e: BackendElectronicDelivery,
): FulfillmentOperation {
    return baseOperation({
        operationId: e.id,
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

function serviceToOperation(
    s: BackendServiceFulfillment,
): FulfillmentOperation {
    return baseOperation({
        operationId: s.id,
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

async function hydrateOperationDetail(
    operation: FulfillmentOperation,
): Promise<FulfillmentOperation> {
    try {
        if (operation.operationType === "RECEIPT") {
            const detail = await apiGet<BackendPurchaseReceiptDetail>(
                `/admin/purchase-receipts/${encodeURIComponent(operation.operationId)}`,
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
                ...operation,
                editVersion: detail.receipt.version,
                sourceVersion: String(detail.receipt.version),
                lines,
                draft: {
                    type: "RECEIPT",
                    warehouseId: detail.receipt.warehouse_id,
                    warehouseLabel: detail.receipt.warehouse_id,
                    occurredAt:
                        operation.draft.type === "RECEIPT"
                            ? operation.draft.occurredAt
                            : nowIso().slice(0, 16),
                    lines: draftLines,
                },
            }
        }
        if (
            operation.operationType === "WAREHOUSE_SHIP" ||
            operation.operationType === "SUPPLIER_DIRECT"
        ) {
            const detail = await apiGet<BackendDeliveryDetail>(
                `/admin/deliveries/${encodeURIComponent(operation.operationId)}`,
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
            if (operation.operationType === "WAREHOUSE_SHIP") {
                return {
                    ...operation,
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
                ...operation,
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
    return operation
}

function filterSummary(
    filters: FulfillmentQueueFilters,
    warehouseOptions: FulfillmentQueueView["context"]["warehouseOptions"],
): string {
    const parts = [
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

function matchOperation(
    operation: FulfillmentOperation,
    filters: FulfillmentQueueFilters,
    roleTypes: readonly FulfillmentOperationType[],
): boolean {
    if (!roleTypes.includes(operation.operationType)) return false
    if (
        filters.operationTypes &&
        filters.operationTypes.length > 0 &&
        !filters.operationTypes.includes(operation.operationType)
    ) {
        return false
    }
    if (filters.warehouseId) {
        if (
            (operation.operationType === "RECEIPT" ||
                operation.operationType === "WAREHOUSE_SHIP") &&
            operation.source.warehouseId !== filters.warehouseId
        ) {
            return false
        }
    }
    if (
        filters.salesOrderId &&
        operation.source.salesOrderId !== filters.salesOrderId
    ) {
        return false
    }
    if (
        filters.purchaseOrderId &&
        operation.source.purchaseOrderId !== filters.purchaseOrderId
    ) {
        return false
    }
    if (filters.q) {
        const q = filters.q.trim().toUpperCase()
        const hay = [
            operation.source.salesOrderNo,
            operation.source.purchaseNo ?? "",
            operation.summary,
            operation.operationId,
        ]
            .join(" ")
            .toUpperCase()
        if (!hay.includes(q)) return false
    }
    if (filters.due === "overdue" && !operation.overdue) return false
    if (filters.due === "today") {
        const today = new Date().toISOString().slice(0, 10)
        if (operation.dueAt.slice(0, 10) !== today) return false
    }
    if (filters.gate === "blocked" && operation.gate.state !== "BLOCKED")
        return false
    if (filters.gate === "satisfied" && operation.gate.state !== "SATISFIED")
        return false
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
            operations: [],
            emptyReason: "NO_PERMISSION",
        }
    }

    // Load draft documents for types visible to role
    const want = new Set(role.types)
    const operations: FulfillmentOperation[] = []

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
                    for (const r of page.items)
                        operations.push(receiptToOperation(r))
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
                        const t = deliveryToOperation(d)
                        if (want.has(t.operationType)) operations.push(t)
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
                    for (const e of page.items)
                        operations.push(electronicToOperation(e))
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
                    for (const s of page.items)
                        operations.push(serviceToOperation(s))
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
        // fall back to operation-derived
        const seen = new Map<string, string>()
        for (const t of operations) {
            const id = t.source.warehouseId
            if (id && !seen.has(id)) seen.set(id, t.source.warehouseLabel ?? id)
        }
        warehouseOptions = [...seen].map(([value, label]) => ({ value, label }))
    }

    const inScope = operations.filter((t) =>
        role.types.includes(t.operationType),
    )
    const metrics = role.types.map((operationType) => ({
        operationType,
        label: `待${OPERATION_TYPE_SHORT[operationType]}`,
        count: inScope.filter((t) => t.operationType === operationType).length,
        visible: true,
    }))

    let filtered = inScope.filter((t) => matchOperation(t, filters, role.types))
    filtered = [...filtered].sort((a, b) => {
        if (a.overdue !== b.overdue) return a.overdue ? -1 : 1
        if (a.priority !== b.priority) return b.priority - a.priority
        return a.dueAt.localeCompare(b.dueAt)
    })

    let position = 0
    let current = filtered[0]
    if (filters.currentOperationId) {
        const idx = filtered.findIndex(
            (t) => t.operationId === filters.currentOperationId,
        )
        if (idx >= 0) {
            position = idx
            current = filtered[idx]
        }
    }

    if (current) {
        current = await hydrateOperationDetail(current)
        filtered = filtered.map((t) =>
            t.operationId === current!.operationId ? current! : t,
        )
    }

    const emptyReason =
        inScope.length === 0
            ? "NO_OPERATIONS"
            : filtered.length === 0
              ? "FILTER_NO_RESULT"
              : undefined

    return {
        preferences: { autoNextDefault: true },
        context: {
            position: filtered.length === 0 ? 0 : position + 1,
            total: filtered.length,
            currentOperationId: current?.operationId,
            previousOperationId: filtered[position - 1]?.operationId,
            nextOperationId: filtered[position + 1]?.operationId,
            filterSummary: filterSummary(filters, warehouseOptions),
            warehouseOptions,
            visibleTypes: role.types,
            roleLabel: role.label,
            viewerLabel: role.userLabel,
            canExecute: role.canExecute,
            snapshotUpdatedAt: nowIso(),
        },
        metrics,
        operations: filtered,
        current,
        emptyReason,
    }
}

export async function saveFulfillmentOperation(
    input: SaveFulfillmentOperationCommand,
): Promise<{ editVersion: number }> {
    const draft = input.draft
    if (draft.type === "RECEIPT") {
        const updated = await apiPut<BackendPurchaseReceipt>(
            `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
                warehouse_id: draft.warehouseId || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    if (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") {
        const updated = await apiPut<BackendDelivery>(
            `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
                carrier: draft.carrier || undefined,
                tracking_no: draft.trackingNo || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    throw new Error("电子交付与服务履约草稿不支持保存；请直接确认正式单据")
}

function formalFromReceipt(
    receipt: BackendPurchaseReceipt,
    draft: Extract<FulfillmentDraft, { type: "RECEIPT" }>,
    operationId: string,
): FulfillmentFormalOutcome {
    return {
        kind: "POSTED",
        operationId,
        factType: "PURCHASE_RECEIPT",
        factId: receipt.id,
        factNo: receipt.receipt_no,
        formalStatus: receipt.status || "POSTED",
        occurredAt:
            secsToIso(receipt.posted_at) || draft.occurredAt || nowIso(),
        operationType: "RECEIPT",
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: false,
        acceptanceNextStep:
            "入库不等于验收。合格的货已入库并按销售单留好；等发货之后，再由销售去登记客户验收。",
        inventoryImpactSummary: "单据已确认；库存影响以库存台账为准。",
        reference: receipt.receipt_no,
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
    operationId: string,
): FulfillmentFormalOutcome {
    const isWh = draft.type === "WAREHOUSE_SHIP"
    return {
        kind: "POSTED",
        operationId,
        factType: "DELIVERY",
        factId: delivery.id,
        factNo: delivery.delivery_no,
        formalStatus: delivery.status || "SHIPPED",
        occurredAt:
            secsToIso(delivery.shipped_at) || draft.shippedAt || nowIso(),
        operationType: draft.type,
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: true,
        acceptanceNextStep: isWh
            ? "仓发记录已确认。物流签收不等于客户验收；请销售在客户验收登记。"
            : "供应商直发记录已确认，不影响自有库存。请销售在客户验收登记（物流签收≠验收）。",
        inventoryImpactSummary: isWh
            ? "发货单已确认；库存与留货影响以库存台账为准。"
            : "供应商直发不影响自有库存。",
        reference: delivery.delivery_no,
        salesOrderId: delivery.sales_order_id,
        salesOrderNo: delivery.sales_order_id,
    }
}

export async function postFulfillmentOperation(
    input: PostFulfillmentOperationCommand,
): Promise<FormalActionResponse> {
    const draft = input.draft

    try {
        if (draft.type === "RECEIPT") {
            // Prefer post existing draft document; if not found, create then post
            const receiptId = input.operationId
            let receipt: BackendPurchaseReceipt | null = null
            try {
                const detail = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
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

            let commandVersion = input.expectedDocumentVersion
            if (
                receipt.warehouse_id !== draft.warehouseId &&
                draft.warehouseId
            ) {
                const updated = await apiPut<BackendPurchaseReceipt>(
                    `/admin/purchase-receipts/${encodeURIComponent(receiptId)}`,
                    {
                        version: input.expectedDocumentVersion,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                        warehouse_id: draft.warehouseId,
                    },
                )
                commandVersion = updated.version
            }

            const posted = await apiPost<BackendPurchaseReceipt>(
                `/admin/purchase-receipts/${encodeURIComponent(receiptId)}/post`,
                {
                    version: commandVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: formalFromReceipt(posted, draft, input.operationId),
            }
        }

        if (
            draft.type === "WAREHOUSE_SHIP" ||
            draft.type === "SUPPLIER_DIRECT"
        ) {
            let delivery: BackendDelivery | null = null
            try {
                const detail = await apiGet<BackendDeliveryDetail>(
                    `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
                )
                delivery = detail.delivery
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) throw error
            }

            if (!delivery) {
                return {
                    status: "failed",
                    code: "DOCUMENT_NOT_FOUND",
                    message: "发货草稿已不存在，请刷新后重新选择单据",
                }
            }

            const updated = await apiPut<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                    carrier: draft.carrier || undefined,
                    tracking_no: draft.trackingNo || undefined,
                },
            )
            const posted = await apiPost<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}/post`,
                {
                    version: updated.version,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: formalFromDelivery(posted, draft, input.operationId),
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
            const confirmed = await apiPost<BackendElectronicDelivery>(
                `/admin/electronic-deliveries/${encodeURIComponent(input.operationId)}/confirm`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: {
                    kind: "POSTED",
                    operationId: input.operationId,
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
        const confirmed = await apiPost<BackendServiceFulfillment>(
            `/admin/service-fulfillments/${encodeURIComponent(input.operationId)}/confirm`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            outcome: {
                kind: "POSTED",
                operationId: input.operationId,
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
                    idempotencyKey: input.idempotencyKey,
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

export async function resolveUnknownFulfillmentResult(
    input: ResolveFulfillmentOperationCommand,
): Promise<FormalActionResponse> {
    // Probe document status for posted outcomes
    const probes: Array<() => Promise<FormalActionResponse | null>> = [
        async () => {
            try {
                const d = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
                )
                if (d.receipt.status === "POSTED") {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
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
                    `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
                )
                if (
                    d.delivery.status === "SHIPPED" ||
                    d.delivery.status === "SIGNED"
                ) {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
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
        message: "未找到该单据对应的处理中请求",
    }
}
