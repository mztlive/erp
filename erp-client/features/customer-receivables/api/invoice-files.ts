/** 开票附件与登记命令使用同一请求提交。 */

import { apiPost, apiPostForm, createApiError } from "@/lib/api"
import {
    invoiceFileReference,
    invoiceFilesError,
} from "@/features/customer-receivables/lib/invoice-files"
import type { BackendInvoice } from "./dto"

/** 有新增附件时上传文件；无附件时沿用 JSON 登记合同。 */
export function postInvoiceCommit(
    command: object,
    files: readonly File[],
): Promise<BackendInvoice> {
    const message = invoiceFilesError(files)
    if (message) {
        return Promise.reject(
            createApiError({ kind: "Validation", message, retryable: false }),
        )
    }
    if (!files.length) {
        return apiPost<BackendInvoice>("/admin/invoices/commit", command)
    }
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            ...command,
            attachment_asset_ids: files.map(invoiceFileReference),
        }),
    )
    for (const file of files) {
        form.append(invoiceFileReference(file), file, file.name)
    }
    return apiPostForm<BackendInvoice>(
        "/admin/invoices/commit-with-files",
        form,
        { timeoutMs: 60_000 },
    )
}
