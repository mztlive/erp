"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SellableItemPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import type { MasterDataListItem } from "@/features/master-data/types"

export function SellablePreviewSheet({
    previewRow,
    lastFocusedRowId,
    onClose,
}: {
    previewRow: MasterDataListItem | null
    lastFocusedRowId: { current: string | null }
    onClose: () => void
}) {
    return (
        <QuickPreviewSheet
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
            title={
                previewRow?.sellableItem
                    ? `${previewRow.name} · ${previewRow.sellableItem.specificationLabel}`
                    : (previewRow?.name ?? "基础资料预览")
            }
            description="公司商品池中当前符合销售资格的 SKU"
            identity={
                previewRow ? (
                    <span className="num">SKU 编号：{previewRow.stableNo}</span>
                ) : null
            }
            summary={
                previewRow?.sellableItem ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="success">当前可售</Badge>
                        <Badge variant="outline">
                            {previewRow.sellableItem.productKindLabel}
                        </Badge>
                        <Badge variant="outline">
                            <span className="num">
                                {previewRow.sellableItem.supplierCount}
                            </span>{" "}
                            家有效供应商
                        </Badge>
                    </div>
                ) : null
            }
            footer={
                previewRow?.sellableItem ? (
                    <>
                        <Button type="button" variant="outline" onClick={onClose}>
                            关闭
                        </Button>
                        <Button
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
