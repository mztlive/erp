"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SheetClose } from "@/components/ui/sheet"
import { StatusBadge } from "@/components/ui/status-badge"
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
                if (!open) onClose()
            }}
            finalFocus={() => {
                const id = lastFocusedRowId.current
                return id
                    ? document.querySelector<HTMLElement>(
                          `[data-row-id="${CSS.escape(id)}"]`,
                      )
                    : null
            }}
            size="preview"
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
                        <StatusBadge tone="success" label="当前可售" />
                        <span className="text-xs text-muted-foreground">
                            {previewRow.sellableItem.productKindLabel}
                        </span>
                    </div>
                ) : null
            }
            footer={
                previewRow?.sellableItem ? (
                    <>
                        <SheetClose
                            id={`${prefix}-close`}
                            render={<Button type="button" variant="outline" />}
                        >
                            关闭
                        </SheetClose>
                        <PermissionGuard permission="product:detail">
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
                        </PermissionGuard>
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
