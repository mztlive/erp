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
    owner_options?: { value: string; label: string }[]
    handler_options?: { value: string; label: string }[]
    as_of?: string
}

function optionLabel(
    id: string | undefined,
    options: ReadonlyArray<{ value: string; label: string }>,
): string | undefined {
    if (!id) return undefined
    return options.find((option) => option.value === id)?.label
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

    const ownerOptions = pageRes.owner_options ?? []
    const handlerOptions = pageRes.handler_options ?? []
    const rows = (pageRes.items ?? []).map((order) => {
        const row = mapListRow(order)
        row.followUpUserName = optionLabel(row.followUpUserId, ownerOptions)
        row.handlerUserName = optionLabel(row.handlerUserId, handlerOptions)
        return row
    })

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
        ownerOptions,
        handlerOptions,
    }
}
