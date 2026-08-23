import { apiGet } from "@/lib/api"
import type {
    PurchaseChangeOrderSummary,
    PurchaseCreationBasis,
    PurchaseOrderCenterView,
    PurchaseOrderListItem,
} from "@/features/purchase-orders/types"
import { isApiError } from "./purchase-order-errors"
import {
    mapBasis,
    mapCenter,
    mapListItem,
    mapPurchaseChangeOrder,
} from "./purchase-order-mapping"
import { metricStatusParam, toBackendStatus } from "./purchase-order-status"
import type {
    BackendBasis,
    BackendCenter,
    BackendListItem,
    BackendPage,
    BackendPurchaseChangeOrder,
} from "./purchase-order-wire-types"
import type {
    PurchaseOrderListQuery,
    PurchaseOrderListResult,
} from "./purchase-orders-contract"

const PURCHASE_ORDER_DEFAULT_PAGE_SIZE = 20
const PURCHASE_ORDER_MAX_PAGE_SIZE = 100

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

    const pageData = await apiGet<BackendPage<BackendListItem>>(
        "/admin/purchase-orders",
        {
            q: query.q,
            sales_order_id: query.salesOrderId,
            status,
            page,
            page_size: pageSize,
            sort_by: sortBy,
            sort_dir: query.sortDir,
        },
    )

    const rows = (pageData.items ?? []).map(mapListItem)

    return {
        rows,
        total: pageData.total ?? rows.length,
        page: pageData.page ?? page,
        pageSize: pageData.page_size ?? pageSize,
        metrics: [],
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

/**
 * 读取采购变更单详情，补齐统一只读审批投影。
 *
 * @param id 变更单 ID。
 */
export async function fetchPurchaseChangeOrderDetail(
    id: string,
): Promise<PurchaseChangeOrderSummary> {
    const detail = await apiGet<BackendPurchaseChangeOrder>(
        `/admin/purchase-change-orders/${encodeURIComponent(id)}`,
    )
    return mapPurchaseChangeOrder(detail)
}

const isOpenPurchaseChangeStatus = (status?: string): boolean =>
    status !== "EFFECTIVE" && status !== "VOIDED" && status !== "VOID"

/**
 * 读取原采购单上尚未终态的采购变更，并补详情审批投影。
 *
 * @param purchaseOrderId 原采购单 ID。
 * @param preferredChangeId 任务或 URL 指定的变更单；缺省取第一条在途改单。
 */
export async function fetchActivePurchaseChangeOrder(
    purchaseOrderId: string,
    preferredChangeId?: string,
): Promise<PurchaseChangeOrderSummary | null> {
    if (preferredChangeId) {
        try {
            const preferred =
                await fetchPurchaseChangeOrderDetail(preferredChangeId)
            if (preferred.purchaseOrderId === purchaseOrderId) {
                return preferred
            }
        } catch (error) {
            if (!(isApiError(error) && error.status === 404)) throw error
        }
    }

    try {
        const page = await apiGet<BackendPage<BackendPurchaseChangeOrder>>(
            "/admin/purchase-change-orders",
            {
                purchase_order_id: purchaseOrderId,
                page: 1,
                page_size: 10,
            },
        )
        const active =
            (page.items ?? []).find(
                (change) =>
                    change.purchase_order_id === purchaseOrderId &&
                    isOpenPurchaseChangeStatus(change.status),
            ) ?? null
        if (!active) return null
        try {
            return await fetchPurchaseChangeOrderDetail(active.id)
        } catch {
            return mapPurchaseChangeOrder(active)
        }
    } catch {
        return null
    }
}

/**
 * 路径若是变更单 ID，回落到原采购单对象中心并挂上该改单。
 *
 * @param changeId 采购变更单 ID。
 */
const loadCenterFromChangeOrder = async (
    changeId: string,
): Promise<PurchaseOrderCenterView | null> => {
    try {
        const change = await fetchPurchaseChangeOrderDetail(changeId)
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(change.purchaseOrderId)}`,
        )
        return {
            ...mapCenter(center),
            activeChangeOrder: change,
        }
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }
}

/**
 * 读取采购单对象中心，并挂上在途采购变更单的只读审批投影。
 *
 * 路径若是变更单 ID，回落到原采购单详情。
 *
 * @param purchaseOrderId 采购单或变更单 ID。
 * @param options.changeOrderId 任务或 URL 指定的变更单。
 */
export async function fetchPurchaseOrderCenter(
    purchaseOrderId: string,
    options?: { changeOrderId?: string },
): Promise<PurchaseOrderCenterView | null> {
    try {
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}`,
        )
        const view = mapCenter(center)
        const activeChangeOrder = await fetchActivePurchaseChangeOrder(
            center.id,
            options?.changeOrderId,
        )
        return { ...view, activeChangeOrder }
    } catch (error) {
        if (isApiError(error) && error.status === 404) {
            return loadCenterFromChangeOrder(purchaseOrderId)
        }
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
