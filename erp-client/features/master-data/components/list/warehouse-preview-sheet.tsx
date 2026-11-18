"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import Link from "next/link"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { MasterDataPreviewPanel } from "@/features/master-data/components/list/master-data-preview"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { restoreListRowFocus } from "@/lib/list-row-focus"
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
    canReadRevisions,
}: {
    previewRow: MasterDataListItem | null
    lastFocusedRowId: { current: string | null }
    previewDetail: MasterDataCenterView | null | undefined
    previewDetailLoading: boolean
    onClose: () => void
    onRevise: (row: MasterDataListItem) => void
    onDisable: (row: MasterDataListItem) => void
    canMaintainHandlers: boolean
    canReadRevisions: boolean
}) {
    const previewIdSegment = previewRow
        ? toAutomationIdSegment(previewRow.stableId)
        : "preview"
    return (
        <QuickPreviewSheet
            idPrefix="master-data-warehouse-preview-sheet"
            open={previewRow != null}
            onOpenChange={(open) => {
                if (!open) {
                    onClose()
                    restoreListRowFocus(lastFocusedRowId.current)
                }
            }}
            size="preview"
            title={previewRow?.name ?? "基础资料预览"}
            identity={
                previewRow ? (
                    <span className="num">
                        {previewRow.stableNo}
                        {canReadRevisions
                            ? ` · v${previewRow.revisionNo}`
                            : null}
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
                        {canReadRevisions ? (
                            <Badge
                                variant={
                                    previewRow.revisionTiming === "FUTURE"
                                        ? "warning"
                                        : "secondary"
                                }
                            >
                                {previewRow.revisionTimingLabel}
                            </Badge>
                        ) : null}
                    </div>
                ) : null
            }
            footer={
                previewRow ? (
                    <>
                        <Button
                            id={`master-data-warehouse-preview-${previewIdSegment}-close`}
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {canMaintainHandlers ? (
                            <Button
                                id={`master-data-warehouse-preview-${previewIdSegment}-revise-handlers`}
                                type="button"
                                variant="outline"
                                disabled={
                                    !previewRow.allowedActions.includes(
                                        "MAINTAIN_FULFILLMENT_HANDLERS",
                                    )
                                }
                                onClick={() => onRevise(previewRow)}
                            >
                                配置收发责任
                            </Button>
                        ) : null}
                        <PermissionGuard permission="warehouse:update">
                            <DisabledActionHint
                                message={
                                    previewRow.actionBlockers.find(
                                        (blocker) =>
                                            blocker.action === "DISABLE",
                                    )?.message
                                }
                            >
                                <Button
                                    id={`master-data-warehouse-preview-${previewIdSegment}-disable`}
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
                        </PermissionGuard>
                        <PermissionGuard permission="warehouse_revision:list">
                            <Button
                                id={`master-data-warehouse-preview-${previewIdSegment}-open-detail`}
                                type="button"
                                render={
                                    <Link
                                        href={`/master-data/warehouses/${previewRow.stableId}?section=overview`}
                                    />
                                }
                            >
                                {masterDataCopy.actionOpenDetail}
                            </Button>
                        </PermissionGuard>
                    </>
                ) : null
            }
        >
            {previewRow ? (
                <MasterDataPreviewPanel
                    row={previewRow}
                    detail={previewDetail}
                    detailLoading={previewDetailLoading}
                    showRevision={canReadRevisions}
                    showActionBlockers={canMaintainHandlers}
                />
            ) : null}
        </QuickPreviewSheet>
    )
}
