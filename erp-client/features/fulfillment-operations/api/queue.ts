/**
 * W09 履约单据处理 · 队列查询：按角色拉取各类 DRAFT 单据，投影为工作单并
 * 在客户端完成筛选/排序/明细补全。权限不匹配时不报错，回退为空队列。
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    FulfillmentOperation,
    FulfillmentOperationType,
    FulfillmentQueueView,
} from "@/features/fulfillment-operations/types"
import { OPERATION_TYPE_SHORT } from "@/features/fulfillment-operations/types"
import {
    resolveRole,
    type FulfillmentRole,
} from "@/features/fulfillment-operations/lib/fulfillment-roles"
import { isApiError, nowIso } from "@/features/fulfillment-operations/lib/projection"
import {
    deliveryToOperation,
    electronicToOperation,
    receiptToOperation,
    serviceToOperation,
    type BackendDelivery,
    type BackendElectronicDelivery,
    type BackendPurchaseReceipt,
    type BackendServiceFulfillment,
    type BackendWarehouse,
} from "./documents"
import { hydrateOperationDetail } from "./hydrate"

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
    /**
     * 按采购单筛选时，反查到的来源销售单。
     * 仓发草稿不挂采购单，要用这个身份才能和入库出现在同一队列。
     */
    linkedSalesOrderId?: string
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
    if (filters.purchaseOrderId) {
        const samePurchaseOrder =
            operation.source.purchaseOrderId === filters.purchaseOrderId
        const warehouseShipForLinkedSales =
            operation.operationType === "WAREHOUSE_SHIP" &&
            Boolean(operation.source.salesOrderId) &&
            Boolean(filters.linkedSalesOrderId) &&
            operation.source.salesOrderId === filters.linkedSalesOrderId
        if (!samePurchaseOrder && !warehouseShipForLinkedSales) {
            return false
        }
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

/**
 * 按采购单反查来源销售单。仓发草稿只挂销售单，入库按采购单筛时要用这个身份衔接。
 *
 * @param filters 当前队列筛选。
 * @returns 已有销售单筛选、采购单上的销售单，或无法解析时的 undefined。
 */
async function resolveLinkedSalesOrderId(
    filters: FulfillmentQueueFilters,
): Promise<string | undefined> {
    if (filters.salesOrderId) return filters.salesOrderId
    if (!filters.purchaseOrderId) return undefined
    try {
        const purchaseOrder = await apiGet<{ sales_order_id?: string }>(
            `/admin/purchase-orders/${encodeURIComponent(filters.purchaseOrderId)}`,
        )
        const salesOrderId = purchaseOrder.sales_order_id?.trim()
        return salesOrderId || undefined
    } catch {
        return undefined
    }
}

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
            preferences: { autoNextDefault: filters.role !== "warehouse" },
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
    const linkedSalesOrderId = await resolveLinkedSalesOrderId(filters)
    const queueFilters: FulfillmentQueueFilters = {
        ...filters,
        linkedSalesOrderId,
    }

    const loaders: Promise<void>[] = []

    if (want.has("RECEIPT")) {
        loaders.push(
            apiGet<Page<BackendPurchaseReceipt>>(
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
            apiGet<Page<BackendDelivery>>("/admin/deliveries", {
                page: 1,
                page_size: 100,
                status: "DRAFT",
                sales_order_id: linkedSalesOrderId ?? filters.salesOrderId,
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
            apiGet<Page<BackendElectronicDelivery>>(
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
            apiGet<Page<BackendServiceFulfillment>>(
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

    if (linkedSalesOrderId) {
        for (let index = 0; index < operations.length; index += 1) {
            const operation = operations[index]
            if (
                operation.operationType === "RECEIPT" &&
                !operation.source.salesOrderId
            ) {
                operations[index] = {
                    ...operation,
                    source: {
                        ...operation.source,
                        salesOrderId: linkedSalesOrderId,
                        salesOrderNo:
                            operation.source.salesOrderNo || linkedSalesOrderId,
                    },
                }
            }
        }
    }

    // warehouse options from warehouses API
    let warehouseOptions: FulfillmentQueueView["context"]["warehouseOptions"] =
        []
    try {
        const wh = await apiGet<Page<BackendWarehouse>>(
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

    let filtered = inScope.filter((t) =>
        matchOperation(t, queueFilters, role.types),
    )
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
        preferences: { autoNextDefault: filters.role !== "warehouse" },
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
