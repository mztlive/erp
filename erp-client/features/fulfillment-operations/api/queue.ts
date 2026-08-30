/**
 * W01/W09 履约任务作业面 · 队列查询：普通队列读取服务端 WorkItem 分页投影；
 * 工作项入口按冻结的业务对象类型和主键精确读取。权限不匹配时返回明确空态。
 */

import { apiGet } from "@/lib/api"
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
    secsToIso,
} from "@/features/fulfillment-operations/lib/projection"
import {
    electronicToOperation,
    serviceToOperation,
    type BackendDeliveryDetail,
    type BackendElectronicDelivery,
    type BackendPurchaseReceiptDetail,
    type BackendServiceFulfillment,
} from "./documents"
import {
    deliveryDetailToOperation,
    hydrateOperationDetail,
    receiptDetailToOperation,
} from "./hydrate"
import {
    decodeFulfillmentQueuePage,
    fulfillmentQueueItemToOperation,
} from "./work-queue"

const DEFAULT_QUEUE_PAGE_SIZE = 20

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
    /** 服务端页码（1 起），同时进入 URL、queryKey 与请求参数。 */
    page?: number
    pageSize?: number
    /** 可选稳定快照身份；不匹配时服务端要求刷新。 */
    queueContextId?: string
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
        if (operation.source.salesOrderId !== filters.salesOrderId) return false
    }
    if (filters.purchaseOrderId) {
        if (operation.source.purchaseOrderId !== filters.purchaseOrderId) {
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
            page: 1,
            pageSize: 1,
            totalPages: 1,
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
                page: filters.page ?? 1,
                pageSize: filters.pageSize ?? DEFAULT_QUEUE_PAGE_SIZE,
                totalPages: 1,
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

    const requestedTypes =
        filters.operationTypes && filters.operationTypes.length > 0
            ? filters.operationTypes
            : role.types
    const page = Math.max(1, Math.trunc(filters.page ?? 1))
    const pageSize = Math.min(
        100,
        Math.max(1, Math.trunc(filters.pageSize ?? DEFAULT_QUEUE_PAGE_SIZE)),
    )
    const response = decodeFulfillmentQueuePage(
        await apiGet<unknown>("/admin/work-items/fulfillment-queue", {
            operation_types: requestedTypes.join(","),
            warehouse_id: filters.warehouseId,
            q: filters.q,
            due: filters.due,
            gate: filters.gate,
            sales_order_id: filters.salesOrderId,
            purchase_order_id: filters.purchaseOrderId,
            queue_context_id: filters.queueContextId,
            timezone: "Asia/Shanghai",
            page,
            page_size: pageSize,
        }),
    )
    let operations = response.items.map(fulfillmentQueueItemToOperation)
    let positionInPage = 0
    let current = operations[0]
    if (filters.currentOperationId) {
        const idx = operations.findIndex(
            (t) => t.operationId === filters.currentOperationId,
        )
        if (idx >= 0) {
            positionInPage = idx
            current = operations[idx]
        }
    }

    if (current) {
        current = await hydrateOperationDetail(current)
        operations = operations.map((t) =>
            t.operationId === current!.operationId ? current! : t,
        )
    }

    const accessibleTypes = response.visible_types
    const hasAppliedFilters = Boolean(
        filters.operationTypes?.length ||
        filters.warehouseId ||
        filters.q ||
        filters.due ||
        filters.gate ||
        filters.salesOrderId ||
        filters.purchaseOrderId,
    )
    const noPermission = accessibleTypes.length === 0
    const emptyReason: FulfillmentQueueView["emptyReason"] = noPermission
        ? "NO_PERMISSION"
        : response.total === 0 && hasAppliedFilters
          ? "FILTER_NO_RESULT"
          : response.total === 0
            ? "NO_OPERATIONS"
            : operations.length === 0
              ? "FILTER_NO_RESULT"
              : undefined
    const warehouseOptions = response.warehouse_options.map((warehouse) => ({
        value: warehouse.id,
        label: warehouse.label,
    }))
    const metrics = accessibleTypes.map((operationType) => ({
        operationType,
        label: `待${OPERATION_TYPE_SHORT[operationType]}`,
        count:
            response.metrics.find(
                (metric) => metric.operation_type === operationType,
            )?.count ?? 0,
        visible: true,
    }))
    const totalPages = Math.max(
        1,
        Math.ceil(response.total / response.page_size),
    )
    const globalPosition =
        operations.length === 0
            ? 0
            : (response.page - 1) * response.page_size + positionInPage + 1

    return {
        preferences: { autoNextDefault: filters.role !== "warehouse" },
        context: {
            position: globalPosition,
            total: response.total,
            page: response.page,
            pageSize: response.page_size,
            totalPages,
            queueContextId: response.queue_context_id,
            currentOperationId: current?.operationId,
            previousOperationId: operations[positionInPage - 1]?.operationId,
            nextOperationId: operations[positionInPage + 1]?.operationId,
            filterSummary: filterSummary(filters, warehouseOptions),
            warehouseOptions,
            visibleTypes: accessibleTypes,
            roleLabel: role.label,
            viewerLabel: role.userLabel,
            canExecute: role.canExecute && accessibleTypes.length > 0,
            snapshotUpdatedAt: secsToIso(response.as_of) || nowIso(),
        },
        metrics,
        operations,
        current,
        emptyReason,
    }
}
