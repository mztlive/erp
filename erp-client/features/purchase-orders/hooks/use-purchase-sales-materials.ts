"use client"

import { useMutation, useQuery } from "@tanstack/react-query"

import {
    downloadPurchaseSalesMaterial,
    fetchPurchaseSalesMaterial,
} from "@/features/purchase-orders/api/purchase-sales-materials"
import { purchaseOrderKeys } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseSourceSalesMaterialView } from "@/features/purchase-orders/types"

/** 预览只使用采购上下文接口；关闭预览后清理文件缓存。 */
export function usePurchaseSalesMaterialPreview(
    purchaseOrderId: string,
    salesRevisionId: string,
    fileAssetId: string,
) {
    return useQuery({
        queryKey: [
            ...purchaseOrderKeys.detail(purchaseOrderId),
            "sales-material",
            salesRevisionId,
            fileAssetId,
        ],
        queryFn: () => fetchPurchaseSalesMaterial(purchaseOrderId, fileAssetId),
        enabled: Boolean(purchaseOrderId && fileAssetId),
        staleTime: 0,
        gcTime: 0,
    })
}

/** 下载错误由资料区就地显示，避免影响采购明细读取。 */
export function useDownloadPurchaseSalesMaterial(purchaseOrderId: string) {
    return useMutation({
        mutationFn: (file: PurchaseSourceSalesMaterialView) =>
            downloadPurchaseSalesMaterial(
                purchaseOrderId,
                file.fileAssetId,
                file.fileName,
            ),
    })
}
