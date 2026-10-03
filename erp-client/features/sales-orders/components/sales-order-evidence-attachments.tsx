"use client"

import { LoadingButton } from "@/components/ui/loading-button"
import { useSalesOrderEvidenceDownload } from "@/features/sales-orders/hooks/use-sales-order-evidence"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 归档开单依据的受控下载入口。 */
export function SalesOrderEvidenceAttachments({
    salesOrderId,
    files,
}: {
    salesOrderId: string
    files: Array<{ id: string; fileName: string }>
}) {
    const download = useSalesOrderEvidenceDownload(salesOrderId)
    if (!files.length) return null
    return (
        <section
            className="space-y-3 rounded-lg border border-border/70 p-4 md:p-5"
            aria-labelledby="sales-order-evidence-heading"
        >
            <h2
                id="sales-order-evidence-heading"
                className="text-lg font-semibold"
            >
                开单凭证
            </h2>
            <ul className="space-y-2">
                {files.map((file) => (
                    <li
                        key={file.id}
                        className="flex flex-wrap items-center justify-between gap-2 text-sm"
                    >
                        <span className="min-w-0 break-all">
                            {file.fileName}
                        </span>
                        <LoadingButton
                            id={`sales-order-evidence-${toAutomationIdSegment(file.id)}-download`}
                            type="button"
                            variant="outline"
                            size="sm"
                            loading={
                                download.isPending &&
                                download.variables?.id === file.id
                            }
                            disabled={download.isPending}
                            onClick={() => download.mutate(file)}
                        >
                            下载
                        </LoadingButton>
                    </li>
                ))}
            </ul>
        </section>
    )
}
