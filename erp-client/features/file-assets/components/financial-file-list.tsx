"use client"
import { DownloadIcon, FileTextIcon } from "lucide-react"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { FinancialFile } from "../api/business-finance-files"

export function FinancialFileList({
    files,
    idPrefix,
    onDownload,
    pendingAssetId,
    showDocumentNumber = true,
    density = "default",
}: {
    files: FinancialFile[]
    idPrefix: string
    onDownload: (file: FinancialFile) => void
    showDocumentNumber?: boolean
    pendingAssetId?: string
    density?: "default" | "compact"
}) {
    return (
        <ul
            className={cn(
                "divide-y divide-border/70",
                density === "compact" && "rounded-md bg-muted/40 px-3",
            )}
        >
            {files.map((file) => (
                <li
                    key={`${file.document_id}:${file.file_asset_id}`}
                    className={cn(
                        "flex items-center justify-between gap-3 py-3",
                        density === "compact" && "py-1.5",
                    )}
                >
                    {density === "compact" ? (
                        <FileTextIcon
                            aria-hidden="true"
                            className="size-4 shrink-0 text-muted-foreground"
                        />
                    ) : null}
                    <div className="min-w-0 flex-1">
                        <p
                            className={
                                showDocumentNumber
                                    ? "truncate text-body-compact"
                                    : "text-body-compact wrap-anywhere"
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
                        variant={density === "compact" ? "ghost" : "outline"}
                        className="shrink-0"
                        aria-label={`下载 ${file.file_name}`}
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
