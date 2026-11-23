"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { SellableItemPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import type { MasterDataListItem } from "@/features/master-data/types"

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
                if (!open) onClose()
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                finalFocus={() => {
                    const id = lastFocusedRowId.current
                    return id
                        ? document.querySelector<HTMLElement>(
                              `[data-row-id="${CSS.escape(id)}"]`,
                          )
                        : null
                }}
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
                <DialogBody>
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
                </DialogBody>
                {previewRow?.sellableItem ? (
                    <DialogFooter>
                        <DialogClose
                            id={`${prefix}-dialog-close`}
                            render={<Button type="button" variant="outline" />}
                        >
                            关闭
                        </DialogClose>
                        <PermissionGuard permission="product:detail">
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
                        </PermissionGuard>
                    </DialogFooter>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
