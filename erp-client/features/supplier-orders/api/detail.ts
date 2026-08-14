/**
 * W26 供应商订单 · 详情查询端点。
 */

import { apiGet } from "@/lib/api"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { mapDetail } from "./mapping"
import type { BackendDetail } from "./wire-types"

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
