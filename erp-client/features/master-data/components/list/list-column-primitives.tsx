"use client"

import {
    BanIcon,
    EyeIcon,
    FilePenLineIcon,
    UserRoundCogIcon,
} from "lucide-react"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, TableRowActions } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function stableNoColumn(): ColumnDef<MasterDataListItem> {
    return {
        id: "stableNo",
        accessorKey: "stableNo",
        header: masterDataCopy.colStableNo,
        meta: {
            label: masterDataCopy.colStableNo,
            width: "default",
        },
        cell: ({ row }) => (
            <span className="num text-sm">{row.original.stableNo}</span>
        ),
    }
}

export function nameColumn({
    showNumber = false,
}: { showNumber?: boolean } = {}): ColumnDef<MasterDataListItem> {
    return {
        id: "name",
        accessorKey: "name",
        header: masterDataCopy.colName,
        meta: { label: masterDataCopy.colName },
        cell: ({ row }) => (
            <div className="min-w-0">
                <div className="truncate text-sm font-medium">
                    {row.original.name}
                </div>
                {showNumber ? (
                    <div
                        className="num truncate text-xs text-muted-foreground"
                        title={row.original.stableNo}
                    >
                        {row.original.stableNo}
                    </div>
                ) : row.original.keyFacts[0] ? (
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.keyFacts[0].label}：
                        {row.original.keyFacts[0].value}
                    </div>
                ) : null}
            </div>
        ),
    }
}

export function revisionNoColumn(): ColumnDef<MasterDataListItem> {
    return {
        id: "revisionNo",
        header: masterDataCopy.colVersion,
        meta: {
            label: masterDataCopy.colVersion,
            width: "amount",
        },
        cell: ({ row }) => (
            <span className="num text-sm">v{row.original.revisionNo}</span>
        ),
    }
}

export function lifecycleColumn(): ColumnDef<MasterDataListItem> {
    return {
        id: "lifecycle",
        header: masterDataCopy.colLifecycle,
        meta: { label: masterDataCopy.colLifecycle },
        cell: ({ row }) => (
            <div className="flex flex-col gap-1">
                <BusinessStatusBadge
                    context="list"
                    label={row.original.lifecycleStatusLabel}
                    tone={row.original.lifecycleTone}
                />
                {row.original.primaryBlocker ? (
                    <span className="max-w-48 whitespace-normal text-xs text-destructive">
                        {row.original.primaryBlocker}
                    </span>
                ) : null}
                {row.original.scheduledLifecycleLabel ? (
                    <span className="text-xs text-muted-foreground">
                        {row.original.scheduledLifecycleLabel}
                    </span>
                ) : null}
            </div>
        ),
    }
}

export function revisionTimingColumn(): ColumnDef<MasterDataListItem> {
    return {
        id: "revisionTiming",
        header: masterDataCopy.colVersionState,
        meta: { label: masterDataCopy.colVersionState },
        cell: ({ row }) => (
            <Badge
                variant={
                    row.original.revisionTiming === "FUTURE"
                        ? "warning"
                        : "secondary"
                }
            >
                {row.original.revisionTimingLabel}
            </Badge>
        ),
    }
}

export function effectivePeriodColumn(): ColumnDef<MasterDataListItem> {
    return {
        id: "period",
        header: masterDataCopy.colEffective,
        meta: { label: masterDataCopy.colEffective },
        cell: ({ row }) => (
            <span className="num text-[13px]">
                {formatEffectiveRange(
                    row.original.effectiveFrom,
                    row.original.effectiveTo,
                )}
            </span>
        ),
    }
}

export function blockerColumn(
    rows: readonly MasterDataListItem[],
): ColumnDef<MasterDataListItem>[] {
    if (!rows.some((row) => row.primaryBlocker)) return []
    return [
        {
            id: "blocker",
            header: masterDataCopy.colBlocker,
            meta: { label: masterDataCopy.colBlocker },
            cell: ({ row }) =>
                row.original.primaryBlocker ? (
                    <span className="text-xs text-destructive">
                        {row.original.primaryBlocker}
                    </span>
                ) : (
                    <span className="text-xs text-muted-foreground">—</span>
                ),
        },
    ]
}

type ActionColumnInput = {
    lastFocusedRowId: React.MutableRefObject<string | null>
    onReviseTarget?: (item: MasterDataListItem) => void
    onDisableTarget?: (item: MasterDataListItem) => void
    onPreview?: (stableId: string) => void
    onOpen?: (item: MasterDataListItem) => void
}

function markFocused(
    lastFocusedRowId: React.MutableRefObject<string | null>,
    item: MasterDataListItem,
) {
    lastFocusedRowId.current = item.stableId
}

export function disableOnlyActionsColumn({
    lastFocusedRowId,
    onDisableTarget,
}: ActionColumnInput): ColumnDef<MasterDataListItem> {
    return {
        id: "actions",
        size: 104,
        minSize: 104,
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions, align: "end" },
        cell: ({ row }) => {
            const item = row.original
            const segment = toAutomationIdSegment(item.stableId)
            const canDisable = item.allowedActions.includes("DISABLE")
            const disableBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "DISABLE",
            )
            return (
                <TableRowActions
                    moreId={`master-data-list-row-${segment}-more`}
                    moreLabel={`${item.name} 更多操作`}
                    actions={[
                        {
                            id: `master-data-list-row-${segment}-disable`,
                            label: masterDataCopy.actionDisable,
                            icon: BanIcon,
                            disabled: !canDisable,
                            disabledReason: disableBlocker?.message,
                            destructive: true,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                onDisableTarget?.(item)
                            },
                        },
                    ]}
                />
            )
        },
    }
}

export function updateOnlyActionsColumn({
    lastFocusedRowId,
    onReviseTarget,
}: ActionColumnInput): ColumnDef<MasterDataListItem> {
    return {
        id: "actions",
        size: 104,
        minSize: 104,
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions, align: "end" },
        cell: ({ row }) => {
            const item = row.original
            const segment = toAutomationIdSegment(item.stableId)
            const canRevise = item.allowedActions.includes("CREATE_REVISION")
            const reviseBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "CREATE_REVISION",
            )
            return (
                <TableRowActions
                    moreId={`master-data-list-row-${segment}-more`}
                    moreLabel={`${item.name} 更多操作`}
                    actions={[
                        {
                            id: `master-data-list-row-${segment}-revise`,
                            label: masterDataCopy.actionUpdate,
                            icon: FilePenLineIcon,
                            disabled: !canRevise,
                            disabledReason: reviseBlocker?.message,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                onReviseTarget?.(item)
                            },
                        },
                    ]}
                />
            )
        },
    }
}

export function fullActionsColumn({
    lastFocusedRowId,
    onReviseTarget,
    onDisableTarget,
    onPreview,
    onOpen,
}: ActionColumnInput): ColumnDef<MasterDataListItem> {
    return {
        id: "actions",
        size: 240,
        minSize: 240,
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions, align: "end" },
        cell: ({ row }) => {
            const item = row.original
            const segment = toAutomationIdSegment(item.stableId)
            const canRevise = item.allowedActions.includes("CREATE_REVISION")
            const canDisable = item.allowedActions.includes("DISABLE")
            const reviseBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "CREATE_REVISION",
            )
            const disableBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "DISABLE",
            )
            return (
                <TableRowActions
                    moreId={`master-data-list-row-${segment}-more`}
                    moreLabel={`${item.name} 更多操作`}
                    actions={[
                        {
                            id: `master-data-list-row-${segment}-view`,
                            label: masterDataCopy.actionView,
                            icon: EyeIcon,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                if (onOpen) onOpen(item)
                                else onPreview?.(item.stableId)
                            },
                        },
                        {
                            id: `master-data-list-row-${segment}-revise`,
                            label: masterDataCopy.actionUpdate,
                            icon: FilePenLineIcon,
                            disabled: !canRevise,
                            disabledReason: reviseBlocker?.message,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                if (onOpen) onOpen(item)
                                else onReviseTarget?.(item)
                            },
                        },
                        {
                            id: `master-data-list-row-${segment}-disable`,
                            label: masterDataCopy.actionDisable,
                            icon: BanIcon,
                            disabled: !canDisable,
                            disabledReason: disableBlocker?.message,
                            destructive: true,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                onDisableTarget?.(item)
                            },
                        },
                    ]}
                />
            )
        },
    }
}

/** 仓库专用动作：查看与收发责任配置，不冒充仓库资料修订。 */
export function warehouseActionsColumn({
    lastFocusedRowId,
    onPreview,
    onReviseTarget,
    canMaintainHandlers,
}: ActionColumnInput & {
    canMaintainHandlers: boolean
}): ColumnDef<MasterDataListItem> {
    return {
        id: "actions",
        size: 232,
        minSize: 232,
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions, align: "end" },
        cell: ({ row }) => {
            const item = row.original
            const segment = toAutomationIdSegment(item.stableId)
            const allowed =
                canMaintainHandlers &&
                item.allowedActions.includes("MAINTAIN_FULFILLMENT_HANDLERS")
            const message = canMaintainHandlers
                ? undefined
                : "当前账号没有仓库更新权限"
            return (
                <TableRowActions
                    moreId={`master-data-warehouse-row-${segment}-more`}
                    moreLabel={`${item.name} 更多操作`}
                    actions={[
                        {
                            id: `master-data-list-row-${segment}-view`,
                            label: masterDataCopy.actionView,
                            icon: EyeIcon,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                onPreview?.(item.stableId)
                            },
                        },
                        {
                            id: `master-data-warehouse-row-${segment}-handlers`,
                            label: "配置收发责任",
                            icon: UserRoundCogIcon,
                            disabled: !allowed,
                            disabledReason: message,
                            onClick: () => {
                                markFocused(lastFocusedRowId, item)
                                onReviseTarget?.(item)
                            },
                        },
                    ]}
                />
            )
        },
    }
}
