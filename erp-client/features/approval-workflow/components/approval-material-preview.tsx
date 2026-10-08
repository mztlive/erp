"use client"

import Image from "next/image"
import { useEffect, useState } from "react"
import { BusinessFailureState } from "@/components/business"
import { useApprovalMaterialPreview } from "../hooks/use-approval-materials"

/** 独立页面读取审批授权附件；页面卸载时释放临时文件地址。 */
export function ApprovalMaterialPreview({
    instanceId,
    assetId,
    fileName,
}: {
    instanceId: string
    assetId: string
    fileName: string
}) {
    const query = useApprovalMaterialPreview(instanceId, assetId)
    const [url, setUrl] = useState<string | null>(null)
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => URL.revokeObjectURL(next)
    }, [query.data])
    return (
        <main className="flex h-svh flex-col bg-background">
            <header className="shrink-0 border-b px-6 py-4">
                <h1 className="break-all text-lg font-semibold">{fileName}</h1>
            </header>
            <div className="relative min-h-0 flex-1">
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
                ) : query.data?.type === "application/pdf" ? (
                    <iframe
                        id="approval-material-preview-document"
                        src={url}
                        title={fileName}
                        className="h-full w-full border-0"
                    />
                ) : (
                    <div className="relative h-full w-full">
                        <Image
                            src={url}
                            alt={fileName}
                            fill
                            unoptimized
                            sizes="100vw"
                            className="rounded-md object-contain"
                        />
                    </div>
                )}
            </div>
        </main>
    )
}
