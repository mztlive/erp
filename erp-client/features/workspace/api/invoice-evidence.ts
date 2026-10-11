import { apiPostForm } from "@/lib/api"
import { invoiceFileReference } from "@/features/customer-receivables/lib/invoice-files"

/** 只发送任务、版本和附件；开票金额及绑定应收由服务端确定。 */
export function completeInvoice(
    command: {
        work_item_id: string
        expected_task_version: string
        expected_request_version: number
        idempotency_key: string
    },
    files: readonly File[],
) {
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            ...command,
            attachment_asset_ids: files.map(invoiceFileReference),
        }),
    )
    for (const file of files)
        form.append(invoiceFileReference(file), file, file.name)
    return apiPostForm<{ id: string }>(
        "/admin/invoices/complete-with-files",
        form,
        { timeoutMs: 60_000 },
    )
}
