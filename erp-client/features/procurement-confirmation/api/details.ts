/** 采购确认详情与销售单详情的 HTTP 读取（W07 适配层内部使用）。 */

import { apiGet } from "@/lib/api"

import { isApiError } from "./errors"
import type {
    BackendConfirmationDetail,
    BackendSalesOrderDetail,
} from "./backend-types"

export async function fetchConfirmationDetail(
    confirmationId: string,
    workItemId?: string,
): Promise<BackendConfirmationDetail | null> {
    try {
        return await apiGet<BackendConfirmationDetail>(
            `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}`,
            { work_item_id: workItemId },
        )
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }
}

export async function fetchSalesOrderDetail(
    salesOrderId: string,
): Promise<BackendSalesOrderDetail> {
    return apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
    )
}
