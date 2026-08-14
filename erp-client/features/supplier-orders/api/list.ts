/**
 * W26 供应商订单 · 列表查询端点。
 * 后端未提供 view/actionable 筛选，客户端按视图投影。
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    SupplierOrderListQuery,
    SupplierOrderListResult,
} from "@/features/supplier-orders/types"
import {
    emptyMetrics,
    filterSummary,
    mapListRow,
    PERMISSION_VERSION,
} from "./mapping"
import type { BackendOrder } from "./wire-types"

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
