"use client"

import { FileIcon, FileTextIcon, ImageIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api"
import { useDownloadApprovalMaterial } from "../hooks/use-approval-materials"
import type { ApprovalMaterials } from "../api/materials"

const READABLE_MATERIAL_TYPES = new Set([
    "image/jpeg",
    "image/png",
    "image/webp",
    "application/pdf",
])

const materialKind = (contentType: string) =>
    contentType === "application/pdf"
        ? { label: "PDF", Icon: FileTextIcon }
        : contentType.startsWith("image/")
          ? { label: "图片", Icon: ImageIcon }
          : { label: "文件", Icon: FileIcon }

const formatFileSize = (size: number) =>
    size < 1024 * 1024
        ? `${Math.max(1, Math.ceil(size / 1024))} KB`
        : `${(size / (1024 * 1024)).toFixed(1)} MB`

/** 文件目录与内容读取使用同一实例和来源版本。 */
export function ApprovalMaterialFiles({
    instanceId,
    attachments,
    sourceRevisionId,
    idPrefix = "approval-submitted-material",
    emptyMessage = "此次提交未保留可读取的附件。",
}: {
    instanceId: string
    attachments: ApprovalMaterials["attachments"]
    sourceRevisionId?: string
    idPrefix?: string
    emptyMessage?: string
}) {
    const download = useDownloadApprovalMaterial(instanceId, sourceRevisionId)
    return (
        <>
            {attachments.length ? (
                <ul className="divide-y rounded-lg border">
                    {attachments.map((file) => {
                        const kind = materialKind(file.content_type)
                        return (
                            <li
                                key={file.file_asset_id}
                                className="flex flex-wrap items-center gap-3 px-4 py-3 text-sm"
                            >
                                <span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                                    <kind.Icon
                                        aria-hidden="true"
                                        className="size-4"
                                    />
                                </span>
                                <span className="min-w-0 flex-1">
                                    <span className="block break-all font-medium">
                                        {file.file_name}
                                    </span>
                                    <span className="num block text-xs text-muted-foreground">
                                        {kind.label} ·{" "}
                                        {formatFileSize(file.byte_size)}
                                    </span>
                                </span>
                                <div className="flex shrink-0 gap-2">
                                    {READABLE_MATERIAL_TYPES.has(
                                        file.content_type,
                                    ) && (
                                        <Button
                                            id={`${idPrefix}-${toAutomationIdSegment(file.file_asset_id)}-preview`}
                                            variant="ghost"
                                            size="sm"
                                            render={
                                                <a
                                                    href={`/approval-materials?${new URLSearchParams({ instance: instanceId, file: file.file_asset_id, name: file.file_name, ...(sourceRevisionId ? { source_revision_id: sourceRevisionId } : {}) })}`}
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    aria-label={`查看 ${file.file_name}（新标签页）`}
                                                />
                                            }
                                        >
                                            查看
                                        </Button>
                                    )}
                                    <LoadingButton
                                        loading={
                                            download.isPending &&
                                            download.variables
                                                ?.file_asset_id ===
                                                file.file_asset_id
                                        }
                                        id={`${idPrefix}-${toAutomationIdSegment(file.file_asset_id)}-download`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={download.isPending}
                                        onClick={() => download.mutate(file)}
                                    >
                                        {download.isPending &&
                                        download.variables?.file_asset_id ===
                                            file.file_asset_id
                                            ? "读取中…"
                                            : "下载"}
                                    </LoadingButton>
                                </div>
                            </li>
                        )
                    })}
                </ul>
            ) : (
                <p className="rounded-lg border border-dashed px-4 py-3 text-sm text-muted-foreground">
                    {emptyMessage}
                </p>
            )}
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "附件读取失败，请重试")}
                </p>
            )}
        </>
    )
}
