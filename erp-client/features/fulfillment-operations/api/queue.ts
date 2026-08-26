/**
 * W01 履约任务作业面 · 队列查询：普通聚合视图按角色拉取 DRAFT 单据；
 * 工作项入口按冻结的业务对象类型和主键精确读取。权限不匹配时返回明确空态。
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
import {
    isApiError,
    nowIso,
} from "@/features/fulfillment-operations/lib/projection"
import {
    deliveryToOperation,
    electronicToOperation,
    receiptToOperation,
    serviceToOperation,
    type BackendDelivery,
    type BackendDeliveryDetail,
    type BackendElectronicDelivery,
    type BackendPurchaseReceipt,
    type BackendPurchaseReceiptDetail,
    type BackendServiceFulfillment,
    type BackendWarehouse,
} from "./documents"
import {
    deliveryDetailToOperation,
    hydrateOperationDetail,
    receiptDetailToOperation,
} from "./hydrate"

const QUEUE_PAGE_SIZE = 100
const MAX_QUEUE_PAGES = 50

async function loadAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<T[]> {
    const items: T[] = []
    let page = 1
    let total = Number.POSITIVE_INFINITY

    while (items.length < total && page <= MAX_QUEUE_PAGES) {
        const result = await apiGet<Page<T>>(path, {
            ...query,
            page,
            page_size: QUEUE_PAGE_SIZE,
        })
        items.push(...(result.items ?? []))
        total = result.total ?? items.length
        if (!result.items?.length) break
        page += 1
    }

    return items
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
    /** W01 任务只允许加载该任务绑定的履约对象。 */
    operationId?: string
    currentOperationId?: string
    /**
     * 按采购单筛选时，反查到的来源销售单。
     * 仓发草稿不挂采购单，要用这个身份才能和入库出现在同一队列。
     */
    linkedSalesOrderId?: string
    /** 销售单详情聚合履约时，用来源采购单补齐未直接携带销售单 ID 的单据。 */
    linkedPurchaseOrderIds?: readonly string[]
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
    if (filters.operationId && operation.operationId !== filters.operationId) {
        return false
    }
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
    if (filters.salesOrderId) {
        const sameSalesOrder =
            operation.source.salesOrderId === filters.salesOrderId
        const linkedByPurchaseOrder = Boolean(
            operation.source.purchaseOrderId &&
            filters.linkedPurchaseOrderIds?.includes(
                operation.source.purchaseOrderId,
            ),
        )
        if (!sameSalesOrder && !linkedByPurchaseOrder) return false
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

async function resolveLinkedPurchaseOrderIds(
    salesOrderId: string | undefined,
): Promise<readonly string[]> {
    if (!salesOrderId) return []
    try {
        const orders = await loadAllPages<{ id: string }>(
            "/admin/purchase-orders",
            {
                sales_order_id: salesOrderId,
            },
        )
        return orders.map((order) => order.id)
    } catch {
        return []
    }
}

type ResolvedFulfillmentRole = ReturnType<typeof resolveRole>

async function loadExactOperation(
    operationId: string,
    operationType: FulfillmentOperationType,
): Promise<FulfillmentOperation | undefined> {
    const encodedId = encodeURIComponent(operationId)
    switch (operationType) {
        case "RECEIPT": {
            const detail = await apiGet<BackendPurchaseReceiptDetail>(
                `/admin/purchase-receipts/${encodedId}`,
            )
            return detail.receipt.status === "DRAFT"
                ? await receiptDetailToOperation(detail)
                : undefined
        }
        case "WAREHOUSE_SHIP":
        case "SUPPLIER_DIRECT": {
            const detail = await apiGet<BackendDeliveryDetail>(
                `/admin/deliveries/${encodedId}`,
            )
            return detail.delivery.status === "DRAFT"
                ? await deliveryDetailToOperation(detail)
                : undefined
        }
        case "ELECTRONIC": {
            const delivery = await apiGet<BackendElectronicDelivery>(
                `/admin/electronic-deliveries/${encodedId}`,
            )
            return delivery.status === "DRAFT"
                ? await hydrateOperationDetail(electronicToOperation(delivery))
                : undefined
        }
        case "SERVICE": {
            const fulfillment = await apiGet<BackendServiceFulfillment>(
                `/admin/service-fulfillments/${encodedId}`,
            )
            return fulfillment.status === "DRAFT"
                ? await hydrateOperationDetail(serviceToOperation(fulfillment))
                : undefined
        }
    }
}

function exactOperationQueueView(
    filters: FulfillmentQueueFilters,
    role: ResolvedFulfillmentRole,
    operationType: FulfillmentOperationType,
    operation: FulfillmentOperation | undefined,
    emptyReason: FulfillmentQueueView["emptyReason"],
): FulfillmentQueueView {
    const warehouseOptions =
        operation?.source.warehouseId && operation.source.warehouseLabel
            ? [
                  {
                      value: operation.source.warehouseId,
                      label: operation.source.warehouseLabel,
                  },
              ]
            : []
    return {
        preferences: { autoNextDefault: filters.role !== "warehouse" },
        context: {
            position: operation ? 1 : 0,
            total: operation ? 1 : 0,
            currentOperationId: operation?.operationId,
            filterSummary: filterSummary(filters, warehouseOptions),
            warehouseOptions,
            visibleTypes: [operationType],
            roleLabel: role.label,
            viewerLabel: role.userLabel,
            canExecute: role.canExecute,
            snapshotUpdatedAt: nowIso(),
        },
        metrics: [
            {
                operationType,
                label: `待${OPERATION_TYPE_SHORT[operationType]}`,
                count: operation ? 1 : 0,
                visible: true,
            },
        ],
        operations: operation ? [operation] : [],
        current: operation,
        emptyReason: operation ? undefined : emptyReason,
    }
}

async function fetchExactOperationQueue(
    filters: FulfillmentQueueFilters,
    role: ResolvedFulfillmentRole,
    operationType: FulfillmentOperationType,
): Promise<FulfillmentQueueView> {
    try {
        const operation = await loadExactOperation(
            filters.operationId!,
            operationType,
        )
        if (!operation) {
            return exactOperationQueueView(
                filters,
                role,
                operationType,
                undefined,
                "NO_OPERATIONS",
            )
        }
        const hasFrozenIdentity =
            operation.operationId === filters.operationId &&
            operation.operationType === operationType
        const matchesFilters =
            hasFrozenIdentity &&
            matchOperation(operation, filters, [operationType])
        return exactOperationQueueView(
            filters,
            role,
            operationType,
            matchesFilters ? operation : undefined,
            "FILTER_NO_RESULT",
        )
    } catch (error) {
        if (isApiError(error) && error.status === 403) {
            return exactOperationQueueView(
                filters,
                role,
                operationType,
                undefined,
                "NO_PERMISSION",
            )
        }
        throw error
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

    if (filters.operationId) {
        if (!filters.operationTypes || filters.operationTypes.length !== 1) {
            const operationType = role.types[0]
            return exactOperationQueueView(
                filters,
                role,
                operationType,
                undefined,
                "FILTER_NO_RESULT",
            )
        }
        return fetchExactOperationQueue(
            filters,
            role,
            filters.operationTypes[0],
        )
    }

    // Load draft documents for types visible to role
    const want = new Set(role.types)
    const operations: FulfillmentOperation[] = []
    const deniedTypes = new Set<FulfillmentOperationType>()
    const linkedSalesOrderId = await resolveLinkedSalesOrderId(filters)
    const linkedPurchaseOrderIds =
        await resolveLinkedPurchaseOrderIds(linkedSalesOrderId)
    const queueFilters: FulfillmentQueueFilters = {
        ...filters,
        linkedSalesOrderId,
        linkedPurchaseOrderIds,
    }

    const loaders: Promise<void>[] = []

    if (want.has("RECEIPT")) {
        loaders.push(
            loadAllPages<BackendPurchaseReceipt>("/admin/purchase-receipts", {
                status: "DRAFT",
                purchase_order_id: filters.purchaseOrderId,
                sort_by: "created_at",
                sort_dir: "desc",
            })
                .then((items) => {
                    for (const receipt of items) {
                        operations.push(receiptToOperation(receipt))
                    }
                })
                .catch((error) => {
                    if (isApiError(error) && error.status === 403) {
                        deniedTypes.add("RECEIPT")
                        return
                    }
                    throw error
                }),
        )
    }

    if (want.has("WAREHOUSE_SHIP") || want.has("SUPPLIER_DIRECT")) {
        loaders.push(
            loadAllPages<BackendDelivery>("/admin/deliveries", {
                status: "DRAFT",
                sales_order_id: linkedSalesOrderId ?? filters.salesOrderId,
                sort_by: "created_at",
                sort_dir: "desc",
            })
                .then((items) => {
                    for (const delivery of items) {
                        const operation = deliveryToOperation(delivery)
                        if (want.has(operation.operationType)) {
                            operations.push(operation)
                        }
                    }
                })
                .catch((error) => {
                    if (isApiError(error) && error.status === 403) {
                        if (want.has("WAREHOUSE_SHIP")) {
                            deniedTypes.add("WAREHOUSE_SHIP")
                        }
                        if (want.has("SUPPLIER_DIRECT")) {
                            deniedTypes.add("SUPPLIER_DIRECT")
                        }
                        return
                    }
                    throw error
                }),
        )
    }

    if (want.has("ELECTRONIC")) {
        loaders.push(
            loadAllPages<BackendElectronicDelivery>(
                "/admin/electronic-deliveries",
                {
                    status: "DRAFT",
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
                .then((items) => {
                    for (const delivery of items) {
                        operations.push(electronicToOperation(delivery))
                    }
                })
                .catch((error) => {
                    if (isApiError(error) && error.status === 403) {
                        deniedTypes.add("ELECTRONIC")
                        return
                    }
                    throw error
                }),
        )
    }

    if (want.has("SERVICE")) {
        loaders.push(
            loadAllPages<BackendServiceFulfillment>(
                "/admin/service-fulfillments",
                {
                    status: "DRAFT",
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
                .then((items) => {
                    for (const service of items) {
                        operations.push(serviceToOperation(service))
                    }
                })
                .catch((error) => {
                    if (isApiError(error) && error.status === 403) {
                        deniedTypes.add("SERVICE")
                        return
                    }
                    throw error
                }),
        )
    }

    await Promise.all(loaders)

    if (linkedSalesOrderId) {
        for (let index = 0; index < operations.length; index += 1) {
            const operation = operations[index]
            const purchaseOrderId = operation.source.purchaseOrderId
            const belongsToLinkedSalesOrder = Boolean(
                purchaseOrderId &&
                (purchaseOrderId === filters.purchaseOrderId ||
                    linkedPurchaseOrderIds.includes(purchaseOrderId)),
            )
            if (
                operation.operationType === "RECEIPT" &&
                !operation.source.salesOrderId &&
                belongsToLinkedSalesOrder
            ) {
                operations[index] = {
                    ...operation,
                    source: {
                        ...operation.source,
                        salesOrderId: linkedSalesOrderId,
                        salesOrderNo: operation.source.salesOrderNo,
                    },
                }
            }
        }
    }

    // warehouse options from warehouses API
    let warehouseOptions: FulfillmentQueueView["context"]["warehouseOptions"] =
        []
    try {
        const warehouses =
            await loadAllPages<BackendWarehouse>("/admin/warehouses")
        warehouseOptions = warehouses.map((w) => ({
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

    const accessibleTypes = role.types.filter(
        (operationType) => !deniedTypes.has(operationType),
    )
    const inScope = operations.filter((operation) =>
        accessibleTypes.includes(operation.operationType),
    )
    const metrics = accessibleTypes.map((operationType) => ({
        operationType,
        label: `待${OPERATION_TYPE_SHORT[operationType]}`,
        count: inScope.filter((t) => t.operationType === operationType).length,
        visible: true,
    }))

    let filtered = inScope.filter((operation) =>
        matchOperation(operation, queueFilters, accessibleTypes),
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

    const requestedTypes =
        filters.operationTypes && filters.operationTypes.length > 0
            ? filters.operationTypes
            : role.types
    const noPermission = requestedTypes.every((operationType) =>
        deniedTypes.has(operationType),
    )
    const emptyReason = noPermission
        ? "NO_PERMISSION"
        : inScope.length === 0
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
            visibleTypes: accessibleTypes,
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
