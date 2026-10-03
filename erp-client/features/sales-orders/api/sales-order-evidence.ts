import { apiGetBlob } from "@/lib/api"

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
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}
