import { apiGetBlob } from "@/lib/api"
import { downloadBlob } from "@/lib/download-blob"

/** 按销售单来源重新核验凭证关系后下载文件。 */
export async function downloadSalesOrderEvidence(
    salesOrderId: string,
    assetId: string,
    fileName: string,
): Promise<void> {
    const blob = await apiGetBlob(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/evidence-files/${encodeURIComponent(assetId)}/download`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
    downloadBlob(blob, fileName)
}
