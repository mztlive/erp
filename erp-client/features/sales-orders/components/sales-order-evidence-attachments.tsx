"use client"

import { DocumentSection } from "@/components/business"
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
    return (
        <DocumentSection
            title="开单凭证"
            className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
            description="随本单归档的开单依据，供履约与审批核对。"
        >
            {files.length ? (
                <ul className="divide-y divide-border/60">
                    {files.map((file) => (
                        <li
                            key={file.id}
                            className="flex flex-wrap items-center justify-between gap-2 py-3 text-sm first:pt-0 last:pb-0"
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
            ) : (
                <p className="text-sm text-muted-foreground">
                    本单没有开单凭证。
                </p>
            )}
        </DocumentSection>
    )
}
