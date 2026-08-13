"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import {
    MasterDataPreviewPanel,
    SellableItemPreviewPanel,
} from "@/features/master-data/components/list/master-data-preview"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataResource,
} from "@/features/master-data/types"

type MasterDataPreviewSheetProps = {
    skipPreviewSheet: boolean
    previewRow: MasterDataListItem | null | undefined
    lastFocusedRowId: { current: string | null }
    isSellableResource: boolean
    resource: MasterDataResource
    previewDetail: MasterDataCenterView | null | undefined
    previewDetailLoading: boolean
    onClose: () => void
    onRevise: (row: MasterDataListItem) => void
    onDisable: (row: MasterDataListItem) => void
}

function MasterDataPreviewSheet({
    skipPreviewSheet,
    previewRow,
    lastFocusedRowId,
    isSellableResource,
    resource,
    previewDetail,
    previewDetailLoading,
    onClose,
    onRevise,
    onDisable,
}: MasterDataPreviewSheetProps) {
    const previewDetailQuery = {
        data: previewDetail,
        isPending: previewDetailLoading,
    }
    return !skipPreviewSheet ? (
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
            size={isSellableResource ? "preview" : "detail"}
            title={
                previewRow?.sellableItem
                    ? `${previewRow.name} · ${previewRow.sellableItem.specificationLabel}`
                    : (previewRow?.name ?? "基础资料预览")
            }
            description={
                previewRow?.sellableItem
                    ? "公司商品池中当前符合销售资格的 SKU"
                    : undefined
            }
            identity={
                previewRow ? (
                    <span className="num">
                        {previewRow.sellableItem ? "SKU 编号：" : null}
                        {previewRow.stableNo}
                        {!previewRow.sellableItem
                            ? ` · v${previewRow.revisionNo}`
                            : null}
                    </span>
                ) : null
            }
            summary={
                previewRow ? (
                    <div className="flex flex-wrap items-center gap-2">
                        {previewRow.sellableItem ? (
                            <>
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
                            </>
                        ) : (
                            <>
                                <BusinessStatusBadge
                                    context="preview"
                                    label={previewRow.lifecycleStatusLabel}
                                    tone={previewRow.lifecycleTone}
                                />
                                <Badge
                                    variant={
                                        previewRow.revisionTiming === "FUTURE"
                                            ? "warning"
                                            : "secondary"
                                    }
                                >
                                    {previewRow.revisionTimingLabel}
                                </Badge>
                            </>
                        )}
                    </div>
                ) : null
            }
            footer={
                previewRow ? (
                    previewRow.sellableItem ? (
                        <>
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => onClose()}
                            >
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
                    ) : (
                        <>
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => onClose()}
                            >
                                关闭
                            </Button>
                            <DisabledActionHint
                                message={
                                    previewRow.actionBlockers.find(
                                        (b) => b.action === "CREATE_REVISION",
                                    )?.message
                                }
                            >
                                <Button
                                    type="button"
                                    variant="outline"
                                    disabled={
                                        !previewRow.allowedActions.includes(
                                            "CREATE_REVISION",
                                        )
                                    }
                                    title={
                                        previewRow.actionBlockers.find(
                                            (b) =>
                                                b.action === "CREATE_REVISION",
                                        )?.message
                                    }
                                    onClick={() => onRevise(previewRow)}
                                >
                                    {masterDataCopy.actionUpdate}
                                </Button>
                            </DisabledActionHint>
                            <DisabledActionHint
                                message={
                                    previewRow.actionBlockers.find(
                                        (b) => b.action === "DISABLE",
                                    )?.message
                                }
                            >
                                <Button
                                    type="button"
                                    variant="outline"
                                    disabled={
                                        !previewRow.allowedActions.includes(
                                            "DISABLE",
                                        )
                                    }
                                    title={
                                        previewRow.actionBlockers.find(
                                            (b) => b.action === "DISABLE",
                                        )?.message
                                    }
                                    onClick={() => onDisable(previewRow)}
                                >
                                    {masterDataCopy.actionDisable}
                                </Button>
                            </DisabledActionHint>
                            <Button
                                type="button"
                                render={
                                    <Link
                                        href={`/master-data/${resource}/${previewRow.stableId}?section=overview`}
                                    />
                                }
                            >
                                {masterDataCopy.actionOpenDetail}
                            </Button>
                        </>
                    )
                ) : null
            }
        >
            {previewRow ? (
                previewRow.sellableItem ? (
                    <SellableItemPreviewPanel row={previewRow} />
                ) : (
                    <MasterDataPreviewPanel
                        row={previewRow}
                        detail={previewDetailQuery.data}
                        detailLoading={previewDetailQuery.isPending}
                    />
                )
            ) : null}
        </QuickPreviewSheet>
    ) : null
}

export { MasterDataPreviewSheet }
