"use client"

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
                <ul className="space-y-2">
                    {attachments.map((file) => (
                        <li
                            key={file.file_asset_id}
                            className="flex flex-wrap items-center justify-between gap-2 text-sm"
                        >
                            <span className="min-w-0 break-all">
                                {file.file_name}
                            </span>
                            <div className="flex gap-2">
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
                                        download.variables?.file_asset_id ===
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
                    ))}
                </ul>
            ) : (
                <p className="text-sm text-muted-foreground">{emptyMessage}</p>
            )}
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "附件读取失败，请重试")}
                </p>
            )}
        </>
    )
}
