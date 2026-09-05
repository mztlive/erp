"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SellableItemPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import type { MasterDataListItem } from "@/features/master-data/types"

export function SellablePreviewSheet({
    idPrefix,
    previewRow,
    lastFocusedRowId,
    onClose,
}: {
    idPrefix?: string
    previewRow: MasterDataListItem | null
    lastFocusedRowId: { current: string | null }
    onClose: () => void
}) {
    const prefix = idPrefix ?? "master-data-list-sellable-preview-sheet"
    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-sheet`}
            open={previewRow != null}
            onOpenChange={(open) => {
                if (!open) {
                    onClose()
                    if (lastFocusedRowId.current) {
                        const el = document.querySelector(
                            `[data-row-id="${lastFocusedRowId.current}"]`,
                        )
                        if (el instanceof HTMLElement) el.focus()
                    }
                }
            }}
            size="preview"
            overlayClassName="bg-black/20 supports-backdrop-filter:backdrop-blur-none"
            contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px] [&_[data-slot=sheet-header]]:gap-3 [&_[data-slot=sheet-header]]:px-7 [&_[data-slot=sheet-header]]:pt-10 [&_[data-slot=sheet-title]]:text-xl [&_[data-slot=sheet-title]]:leading-8 [&_[data-slot=sheet-title]]:font-semibold [&_[data-slot=quick-preview-identity]]:text-xs [&_[data-slot=quick-preview-content]]:px-7 [&_[data-slot=sheet-footer]]:flex-row [&_[data-slot=sheet-footer]]:justify-end [&_[data-slot=sheet-footer]]:px-7 [&_[data-slot=sheet-footer]]:py-4"
            title={previewRow?.name ?? "商品预览"}
            description={
                previewRow?.sellableItem?.specificationLabel !== "无规格"
                    ? previewRow?.sellableItem?.specificationLabel
                    : undefined
            }
            identity={
                previewRow ? (
                    <span className="num">SKU 编号：{previewRow.stableNo}</span>
                ) : null
            }
            summary={
                previewRow?.sellableItem ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="success">当前可售</Badge>
                        <span className="text-xs text-muted-foreground">
                            {previewRow.sellableItem.productKindLabel}
                        </span>
                    </div>
                ) : null
            }
            footer={
                previewRow?.sellableItem ? (
                    <>
                        <Button
                            id={`${prefix}-close`}
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id={`${prefix}-open-product`}
                            type="button"
                            render={
                                <Link
                                    href={`/master-data/products/${previewRow.sellableItem.productId}?section=overview`}
                                />
                            }
                        >
                            打开商品资料
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden
                            />
                        </Button>
                    </>
                ) : null
            }
        >
            {previewRow?.sellableItem ? (
                <SellableItemPreviewPanel row={previewRow} />
            ) : null}
        </QuickPreviewSheet>
    )
}
