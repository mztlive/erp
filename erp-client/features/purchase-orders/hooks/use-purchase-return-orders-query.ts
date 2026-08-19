"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchPurchaseReturnOrders } from "@/features/purchase-orders/api/purchase-return-orders"

export const purchaseReturnOrderKeys = {
    all: ["purchase-return-orders"] as const,
    list: (purchaseOrderId: string) =>
        [...purchaseReturnOrderKeys.all, "list", purchaseOrderId] as const,
}

/**
 * 读取原采购单关联的采购退货。PurchaseReturnOrder 为 NO_APPROVAL，
 * 查询结果不含审批绑定，不得当作审批待办。
 *
 * @param purchaseOrderId 原采购单 ID；空值时不请求。
 */
export function usePurchaseReturnOrdersQuery(purchaseOrderId: string) {
    return useQuery({
        queryKey: purchaseReturnOrderKeys.list(purchaseOrderId),
        queryFn: () => fetchPurchaseReturnOrders(purchaseOrderId),
        enabled: purchaseOrderId.length > 0,
    })
}
