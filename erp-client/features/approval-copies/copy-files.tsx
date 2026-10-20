"use client"
import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { useCopyFile } from "./queries"
export function CopyFiles({
    id,
    files,
}: {
    id: string
    files: ApprovalMaterials["attachments"]
}) {
    const fileQuery = useCopyFile()
    const [preview, setPreview] = useState<{
        url: string
        name: string
    } | null>(null)
    useEffect(
        () => () => {
            if (preview) URL.revokeObjectURL(preview.url)
        },
        [preview],
    )
    return (
        <div className="space-y-3">
            {!files.length && (
                <p className="text-sm text-muted-foreground">
                    本次提交没有附件。
                </p>
            )}
            {files.map((file) => (
                <div
                    key={file.file_asset_id}
                    className="flex flex-wrap items-center gap-2 rounded-md border p-3"
                >
                    <span className="min-w-0 flex-1 break-all text-sm">
                        {file.file_name}
                    </span>
                    {([false, true] as const).map((isPreview) => {
                        if (
                            isPreview &&
                            ![
                                "application/pdf",
                                "image/jpeg",
                                "image/png",
                                "image/webp",
                            ].includes(file.content_type)
                        )
                            return null
                        return (
                            <Button
                                key={String(isPreview)}
                                id={`copy-file-${toAutomationIdSegment(file.file_asset_id)}-${isPreview ? "preview" : "download"}`}
                                variant="outline"
                                size="sm"
                                disabled={fileQuery.isPending}
                                onClick={() =>
                                    fileQuery.mutate(
                                        {
                                            id,
                                            file: file.file_asset_id,
                                            preview: isPreview,
                                        },
                                        {
                                            onSuccess: (blob) => {
                                                const url =
                                                    URL.createObjectURL(blob)
                                                if (isPreview)
                                                    setPreview({
                                                        url,
                                                        name: file.file_name,
                                                    })
                                                else {
                                                    const link =
                                                        document.createElement(
                                                            "a",
                                                        )
                                                    link.href = url
                                                    link.download =
                                                        file.file_name
                                                    link.click()
                                                    window.setTimeout(
                                                        () =>
                                                            URL.revokeObjectURL(
                                                                url,
                                                            ),
                                                        1000,
                                                    )
                                                }
                                            },
                                        },
                                    )
                                }
                            >
                                {isPreview ? "预览" : "下载"}
                            </Button>
                        )
                    })}
                </div>
            ))}
            {fileQuery.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(fileQuery.error)}
                </p>
            )}
            {preview && (
                <div className="space-y-2">
                    <Button
                        id="copy-file-preview-close"
                        variant="outline"
                        onClick={() => setPreview(null)}
                    >
                        关闭预览
                    </Button>
                    <iframe
                        id="copy-file-preview"
                        title={preview.name}
                        src={preview.url}
                        className="h-[70vh] w-full rounded-md border"
                    />
                </div>
            )}
        </div>
    )
}
