"use client"
import { DownloadIcon } from "lucide-react"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { FinancialFile } from "../api/business-finance-files"

export function FinancialFileList({
    files,
    idPrefix,
    onDownload,
    pendingAssetId,
    showDocumentNumber = true,
}: {
    files: FinancialFile[]
    idPrefix: string
    onDownload: (file: FinancialFile) => void
    showDocumentNumber?: boolean
    pendingAssetId?: string
}) {
    return (
        <ul className="divide-y divide-border/70">
            {files.map((file) => (
                <li
                    key={`${file.document_id}:${file.file_asset_id}`}
                    className="flex items-center justify-between gap-3 py-3"
                >
                    <div className="min-w-0">
                        <p
                            className={
                                showDocumentNumber
                                    ? "truncate text-sm"
                                    : "text-sm wrap-anywhere"
                            }
                        >
                            {file.file_name}
                        </p>
                        {showDocumentNumber ? (
                            <p className="text-xs text-muted-foreground">
                                {file.document_no}
                            </p>
                        ) : null}
                    </div>
                    <LoadingButton
                        id={`${idPrefix}-${toAutomationIdSegment(`${file.document_id}-${file.file_asset_id}`)}-download`}
                        type="button"
                        size="sm"
                        variant="outline"
                        loading={pendingAssetId === file.file_asset_id}
                        disabled={
                            pendingAssetId !== undefined &&
                            pendingAssetId !== file.file_asset_id
                        }
                        onClick={() => onDownload(file)}
                    >
                        <DownloadIcon data-icon="inline-start" />
                        下载
                    </LoadingButton>
                </li>
            ))}
        </ul>
    )
}
