"use client"

import { useMemo, useState } from "react"

import {
    MaterialFileList,
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import {
    fetchApprovalMaterialPreview,
    type ApprovalMaterials,
} from "../api/materials"
import { useDownloadApprovalMaterial } from "../hooks/use-approval-materials"

/** 文件目录与内容读取使用同一实例和来源版本；预览统一留在页内对照或灯箱完成。 */
export function ApprovalMaterialFiles({
    instanceId,
    attachments,
    sourceRevisionId,
    idPrefix = "approval-submitted-material",
    emptyMessage = "此次提交未保留可读取的附件。",
    onSelectFile,
    activeFileId,
}: {
    instanceId: string
    attachments: ApprovalMaterials["attachments"]
    sourceRevisionId?: string
    idPrefix?: string
    emptyMessage?: string
    /** 宽屏右轨对接回调：传入时点击预览切入右侧栏对照，否则弹出统一灯箱预览。 */
    onSelectFile?: (fileId: string) => void
    /** 当前在右轨或灯箱中选中的文件 ID。 */
    activeFileId?: string
}) {
    const download = useDownloadApprovalMaterial(instanceId, sourceRevisionId)
    const [dialogFileId, setDialogFileId] = useState<string | null>(null)

    const items = useMemo<readonly MaterialFileItem[]>(
        () =>
            attachments.map((file) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                sizeBytes: file.byte_size,
                loadPreview: () =>
                    fetchApprovalMaterialPreview(
                        instanceId,
                        file.file_asset_id,
                        sourceRevisionId,
                    ),
                onDownload: () => download.mutateAsync(file),
            })),
        [attachments, download, instanceId, sourceRevisionId],
    )

    const handlePreview = (file: MaterialFileItem) => {
        if (onSelectFile) {
            onSelectFile(file.id)
        } else {
            setDialogFileId(file.id)
        }
    }

    return (
        <>
            <MaterialFileList
                files={items}
                onPreview={handlePreview}
                activeFileId={activeFileId ?? dialogFileId ?? undefined}
                downloadingFileId={
                    download.isPending
                        ? (download.variables?.file_asset_id ?? null)
                        : null
                }
                emptyMessage={emptyMessage}
                idPrefix={idPrefix}
            />
            {!onSelectFile ? (
                <UnifiedMaterialViewer
                    mode="dialog"
                    files={items}
                    activeFileId={dialogFileId ?? undefined}
                    onActiveFileChange={setDialogFileId}
                    open={dialogFileId !== null}
                    onOpenChange={(open) => {
                        if (!open) setDialogFileId(null)
                    }}
                    idPrefix={`${idPrefix}-viewer`}
                />
            ) : null}
        </>
    )
}
