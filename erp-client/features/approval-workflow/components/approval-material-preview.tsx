"use client"

import Image from "next/image"
import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { BusinessFailureState } from "@/components/business"
import type { ApprovalMaterials } from "../api/materials"
import { useApprovalMaterialPreview } from "../hooks/use-approval-materials"

/** 仅渲染审批专门接口返回的图片或 PDF，关闭后释放临时文件地址。 */
export function ApprovalMaterialPreview({
    instanceId,
    file,
    onClose,
}: {
    instanceId: string
    file: ApprovalMaterials["attachments"][number]
    onClose: () => void
}) {
    const query = useApprovalMaterialPreview(instanceId, file.file_asset_id)
    const [url, setUrl] = useState<string | null>(null)
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => URL.revokeObjectURL(next)
    }, [query.data])
    return (
        <section className="space-y-3 border-t pt-5">
            <div className="flex items-center justify-between gap-3">
                <h3 className="min-w-0 break-all font-medium">
                    {file.file_name}
                </h3>
                <Button
                    id="approval-material-preview-close"
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onClose}
                >
                    关闭预览
                </Button>
            </div>
            {query.isError ? (
                <BusinessFailureState
                    id="approval-material-preview-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isFetching || !url ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取附件…
                </p>
            ) : file.content_type === "application/pdf" ? (
                <iframe
                    id="approval-material-preview-document"
                    src={url}
                    title={file.file_name}
                    className="h-[60vh] w-full rounded-md border"
                />
            ) : (
                <div className="relative h-[60vh] w-full">
                    <Image
                        src={url}
                        alt={file.file_name}
                        fill
                        unoptimized
                        sizes="100vw"
                        className="rounded-md object-contain"
                    />
                </div>
            )}
        </section>
    )
}
