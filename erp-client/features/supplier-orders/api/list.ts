/**
 * W26 供应商订单 · 列表查询端点。
 * 视图、取消/退款与人员筛选全部由服务端求交，禁止客户端假分页。
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

type ListEnvelope = Page<BackendOrder> & {
    empty_reason?: string | null
    scope_version?: string
    ownership_basis?: string

    as_of?: string
}

export async function fetchSupplierOrders(
    query: SupplierOrderListQuery,
): Promise<SupplierOrderListResult> {
    const now = new Date().toISOString()
    const pageRes = await apiGet<ListEnvelope>(
        "/admin/supplier-fulfillment-orders",
        {
            page: query.page,
            page_size: query.pageSize,
            supplier_id: query.supplierId,
            fulfillment_status: query.fulfillmentStatuses?.[0],
            cancel_status: query.cancelStatuses?.[0],
            refund_status: query.refundStatuses?.[0],
            q: query.q?.trim() || undefined,
            view: query.view,
            aftersale_pending: query.aftersalePending || undefined,
            owner_user_ids: query.ownerUserIds || undefined,
            handler_user_ids: query.handlerUserIds || undefined,
            org_unit_ids: query.orgUnitIds || undefined,
            include_descendants:
                query.orgUnitIds && query.includeDescendants ? true : undefined,
            scope_version: query.scopeVersion,
            sort_by:
                query.sortBy === "lastBusinessAt" ? "created_at" : "created_at",
            sort_dir: query.sortDir ?? "desc",
        },
    )

    // 跟进人、处理人筛选仍用本列表页内候选。资格不是 role-procurement / role-sales，未接人员目录。
    // 行姓名只读 follow_up_user_name / handler_user_name，不从候选标签回填。

    const rows = (pageRes.items ?? []).map((order) => mapListRow(order))

    return {
        rows,
        pageInfo: {
            page: pageRes.page ?? query.page,
            pageSize: pageRes.page_size ?? query.pageSize,
            total: pageRes.total ?? 0,
        },
        metrics: emptyMetrics(),
        permissionVersion: PERMISSION_VERSION,
        sourceAsOf: pageRes.as_of ?? now,
        queriedAt: now,
        filterSummary: filterSummary(query, pageRes.total ?? 0),
        emptyReason: pageRes.empty_reason,
        scopeVersion: pageRes.scope_version,
        ownershipBasis: pageRes.ownership_basis,
    }
}
