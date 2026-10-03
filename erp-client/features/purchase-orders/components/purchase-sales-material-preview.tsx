"use client"

import Image from "next/image"
import { useEffect, useState } from "react"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { usePurchaseSalesMaterialPreview } from "@/features/purchase-orders/hooks/use-purchase-sales-materials"
import type { PurchaseSourceSalesMaterialView } from "@/features/purchase-orders/types"

/** 合同与凭证预览使用本次授权读取的临时地址，卸载时释放。 */
export function PurchaseSalesMaterialPreview({
    purchaseOrderId,
    salesRevisionId,
    file,
    idPrefix,
    onClose,
}: {
    purchaseOrderId: string
    salesRevisionId: string
    file: PurchaseSourceSalesMaterialView
    idPrefix: string
    onClose: () => void
}) {
    const query = usePurchaseSalesMaterialPreview(
        purchaseOrderId,
        salesRevisionId,
        file.fileAssetId,
    )
    const [url, setUrl] = useState<string | null>(null)
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => URL.revokeObjectURL(next)
    }, [query.data])

    return (
        <section className="space-y-3 border-t pt-5" aria-label="附件预览">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="min-w-0 break-all text-sm font-medium">
                    {file.fileName}
                </h3>
                <Button
                    id={`${idPrefix}-preview-close`}
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
                    id={`${idPrefix}-preview-retry`}
                    title="附件读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isFetching || !url ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取附件…
                </p>
            ) : file.contentType === "application/pdf" ? (
                <iframe
                    id={`${idPrefix}-preview-document`}
                    src={url}
                    title={file.fileName}
                    className="h-[60vh] w-full rounded-md border"
                />
            ) : (
                <div className="relative h-[60vh] w-full">
                    <Image
                        src={url}
                        alt={file.fileName}
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
