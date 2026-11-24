"use client"

import { useMemo, useState } from "react"

import {
    MaterialFileList,
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { downloadBlob } from "@/lib/download-blob"
import { fetchCopyFile } from "./api"
import { useCopyFile } from "./queries"

export function useCopyFileItems(
    id: string,
    files: ApprovalMaterials["attachments"],
) {
    const fileQuery = useCopyFile()
    const { mutateAsync } = fileQuery

    const items = useMemo<readonly MaterialFileItem[]>(
        () =>
            files.map((file) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                sizeBytes: file.byte_size,
                loadPreview: () => fetchCopyFile(id, file.file_asset_id, true),
                onDownload: async () => {
                    const blob = await mutateAsync({
                        id,
                        file: file.file_asset_id,
                        preview: false,
                    })
                    downloadBlob(blob, file.file_name)
                },
            })),
        [files, id, mutateAsync],
    )
    const downloadingFileId =
        fileQuery.isPending && !fileQuery.variables?.preview
            ? (fileQuery.variables?.file ?? null)
            : null

    return { items, downloadingFileId }
}

export function CopyFiles({
    id,
    files,
    onSelectFile,
    activeFileId,
    idPrefix = "copy-file",
}: {
    id: string
    files: ApprovalMaterials["attachments"]
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    idPrefix?: string
}) {
    const { items, downloadingFileId } = useCopyFileItems(id, files)
    const [dialogFileId, setDialogFileId] = useState<string | null>(null)

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
                downloadingFileId={downloadingFileId}
                emptyMessage="本次提交没有附件。"
                idPrefix={idPrefix}
                className="[&>ul]:rounded-none [&>ul]:border-0 [&>ul>li]:px-0"
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
