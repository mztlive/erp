import { apiGetBlob } from "@/lib/api"
import { downloadBlob } from "@/lib/download-blob"

/** 使用采购单来源关系授权读取文件，服务端按关联销售版本重新核验。 */
export function fetchPurchaseSalesMaterial(
    purchaseOrderId: string,
    fileAssetId: string,
): Promise<Blob> {
    return apiGetBlob(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}/sales-materials/${encodeURIComponent(fileAssetId)}/download`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
}

/** 每次下载重新读取采购范围内的关联材料。 */
export async function downloadPurchaseSalesMaterial(
    purchaseOrderId: string,
    fileAssetId: string,
    fileName: string,
): Promise<void> {
    const blob = await fetchPurchaseSalesMaterial(purchaseOrderId, fileAssetId)
    downloadBlob(blob, fileName)
}
