"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { SellableItemPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import type { MasterDataListItem } from "@/features/master-data/types"

function restoreRowFocus(rowId: string | null) {
    if (!rowId) return
    const element = document.querySelector(
        `[data-row-id="${CSS.escape(rowId)}"]`,
    )
    if (element instanceof HTMLElement) element.focus()
}

export function SellablePreviewDialog({
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
    const prefix = idPrefix ?? "master-data-sellable-items-preview"
    const spec =
        previewRow?.sellableItem?.specificationLabel !== "无规格"
            ? previewRow?.sellableItem?.specificationLabel
            : undefined

    return (
        <Dialog
            open={previewRow != null}
            onOpenChange={(open) => {
                if (!open) {
                    onClose()
                    restoreRowFocus(lastFocusedRowId.current)
                }
            }}
        >
            <DialogContent
                className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg"
                closeButtonId={`${prefix}-dialog-dismiss`}
            >
                <DialogHeader>
                    {previewRow ? (
                        <p className="num text-xs text-muted-foreground">
                            SKU 编号：{previewRow.stableNo}
                        </p>
                    ) : null}
                    <DialogTitle>{previewRow?.name ?? "商品预览"}</DialogTitle>
                    {spec ? (
                        <DialogDescription>{spec}</DialogDescription>
                    ) : (
                        <DialogDescription className="sr-only">
                            商品资料
                        </DialogDescription>
                    )}
                    {previewRow?.sellableItem ? (
                        <div className="flex flex-wrap items-center gap-2">
                            <Badge variant="success">当前可售</Badge>
                            <span className="text-xs text-muted-foreground">
                                {previewRow.sellableItem.productKindLabel}
                            </span>
                        </div>
                    ) : null}
                </DialogHeader>
                <DialogScrollBody>
                    {previewRow?.sellableItem ? (
                        <div className="flex flex-col gap-6">
                            <SellableItemThumbnail
                                assetId={
                                    previewRow.sellableItem.mainImageAssetId
                                }
                                label={previewRow.name}
                                className="aspect-[16/9] max-h-48 w-full rounded-xl"
                            />
                            <SellableItemPreviewPanel row={previewRow} />
                        </div>
                    ) : null}
                </DialogScrollBody>
                {previewRow?.sellableItem ? (
                    <DialogFooter>
                        <Button
                            id={`${prefix}-dialog-close`}
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id={`${prefix}-dialog-open-product`}
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
                    </DialogFooter>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
