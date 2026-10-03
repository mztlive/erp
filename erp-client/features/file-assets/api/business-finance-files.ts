/** 财务文件按销售单或正式履约任务访问，不授予通用资产下载资格。 */
import { apiGet, apiGetBlob } from "@/lib/api"

export type FinancialFile = {
    document_id: string
    document_no: string
    file_asset_id: string
    file_name: string
    content_type: string
    byte_size: number
}
export function fetchSalesInvoiceFiles(salesOrderId: string) {
    return apiGet<FinancialFile[]>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/invoice-files`,
    )
}
export function fetchPurchasePaymentReceipts(workItemId: string) {
    return apiGet<FinancialFile[]>(
        `/admin/work-items/${encodeURIComponent(workItemId)}/payment-receipts`,
    )
}

/** 拉取受控响应后触发带用户文件名的浏览器下载。 */
async function download(path: string, fileName: string): Promise<void> {
    const blob = await apiGetBlob(path, {
        timeoutMs: 30_000,
        cache: "no-store",
    })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}
export function downloadSalesInvoiceFile(
    salesOrderId: string,
    file: FinancialFile,
) {
    return download(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/invoice-files/${encodeURIComponent(file.document_id)}/${encodeURIComponent(file.file_asset_id)}/download`,
        file.file_name,
    )
}
export function downloadPurchasePaymentReceipt(
    workItemId: string,
    file: FinancialFile,
) {
    return download(
        `/admin/work-items/${encodeURIComponent(workItemId)}/payment-receipts/${encodeURIComponent(file.document_id)}/download`,
        file.file_name,
    )
}
