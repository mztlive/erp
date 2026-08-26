"use client"

import Link from "next/link"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { MasterDataPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type {
    MasterDataCenterView,
    MasterDataListItem,
} from "@/features/master-data/types"

export function WarehousePreviewSheet({
    previewRow,
    lastFocusedRowId,
    previewDetail,
    previewDetailLoading,
    onClose,
    onRevise,
    onDisable,
    canMaintainHandlers,
}: {
    previewRow: MasterDataListItem | null
    lastFocusedRowId: { current: string | null }
    previewDetail: MasterDataCenterView | null | undefined
    previewDetailLoading: boolean
    onClose: () => void
    onRevise: (row: MasterDataListItem) => void
    onDisable: (row: MasterDataListItem) => void
    canMaintainHandlers: boolean
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
            size="detail"
            title={previewRow?.name ?? "基础资料预览"}
            identity={
                previewRow ? (
                    <span className="num">
                        {previewRow.stableNo} · v{previewRow.revisionNo}
                    </span>
                ) : null
            }
            summary={
                previewRow ? (
                    <div className="flex flex-wrap items-center gap-2">
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
                    </div>
                ) : null
            }
            footer={
                previewRow ? (
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <DisabledActionHint
                            message={
                                canMaintainHandlers
                                    ? undefined
                                    : "当前账号没有仓库更新权限"
                            }
                        >
                            <Button
                                type="button"
                                variant="outline"
                                disabled={
                                    !canMaintainHandlers ||
                                    !previewRow.allowedActions.includes(
                                        "MAINTAIN_FULFILLMENT_HANDLERS",
                                    )
                                }
                                onClick={() => onRevise(previewRow)}
                            >
                                配置收发责任
                            </Button>
                        </DisabledActionHint>
                        <DisabledActionHint
                            message={
                                previewRow.actionBlockers.find(
                                    (blocker) => blocker.action === "DISABLE",
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
                                onClick={() => onDisable(previewRow)}
                            >
                                {masterDataCopy.actionDisable}
                            </Button>
                        </DisabledActionHint>
                        <Button
                            type="button"
                            render={
                                <Link
                                    href={`/master-data/warehouses/${previewRow.stableId}?section=overview`}
                                />
                            }
                        >
                            {masterDataCopy.actionOpenDetail}
                        </Button>
                    </>
                ) : null
            }
        >
            {previewRow ? (
                <MasterDataPreviewPanel
                    row={previewRow}
                    detail={previewDetail}
                    detailLoading={previewDetailLoading}
                />
            ) : null}
        </QuickPreviewSheet>
    )
}
