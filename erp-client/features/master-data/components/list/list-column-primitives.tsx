"use client"

import * as React from "react"
import { BanIcon, HistoryIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataListItem } from "@/features/master-data/types"

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

export function nameColumn(): ColumnDef<MasterDataListItem> {
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
                {row.original.keyFacts[0] ? (
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
                {row.original.scheduledLifecycleLabel ? (
                    <span className="text-tiny text-muted-foreground">
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
            <span className="num text-xs">
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
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions },
        cell: ({ row }) => {
            const item = row.original
            const canDisable = item.allowedActions.includes("DISABLE")
            const disableBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "DISABLE",
            )
            return (
                <div className="flex flex-wrap gap-1">
                    <DisabledActionHint message={disableBlocker?.message}>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={!canDisable}
                            title={disableBlocker?.message}
                            onClick={(event) => {
                                event.stopPropagation()
                                markFocused(lastFocusedRowId, item)
                                onDisableTarget?.(item)
                            }}
                        >
                            <BanIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionDisable}
                        </Button>
                    </DisabledActionHint>
                </div>
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
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions },
        cell: ({ row }) => {
            const item = row.original
            const canRevise = item.allowedActions.includes("CREATE_REVISION")
            const reviseBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "CREATE_REVISION",
            )
            return (
                <div className="flex flex-wrap gap-1">
                    <DisabledActionHint message={reviseBlocker?.message}>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={!canRevise}
                            title={reviseBlocker?.message}
                            onClick={(event) => {
                                event.stopPropagation()
                                markFocused(lastFocusedRowId, item)
                                onReviseTarget?.(item)
                            }}
                        >
                            <HistoryIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionUpdate}
                        </Button>
                    </DisabledActionHint>
                </div>
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
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions },
        cell: ({ row }) => {
            const item = row.original
            const canRevise = item.allowedActions.includes("CREATE_REVISION")
            const canDisable = item.allowedActions.includes("DISABLE")
            const reviseBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "CREATE_REVISION",
            )
            const disableBlocker = item.actionBlockers.find(
                (blocker) => blocker.action === "DISABLE",
            )
            return (
                <div className="flex flex-wrap gap-1">
                    <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        onClick={(event) => {
                            event.stopPropagation()
                            markFocused(lastFocusedRowId, item)
                            if (onOpen) onOpen(item)
                            else onPreview?.(item.stableId)
                        }}
                    >
                        {masterDataCopy.actionView}
                    </Button>
                    <DisabledActionHint message={reviseBlocker?.message}>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={!canRevise}
                            title={reviseBlocker?.message}
                            onClick={(event) => {
                                event.stopPropagation()
                                markFocused(lastFocusedRowId, item)
                                if (onOpen) onOpen(item)
                                else onReviseTarget?.(item)
                            }}
                        >
                            <HistoryIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionUpdate}
                        </Button>
                    </DisabledActionHint>
                    <DisabledActionHint message={disableBlocker?.message}>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={!canDisable}
                            title={disableBlocker?.message}
                            onClick={(event) => {
                                event.stopPropagation()
                                markFocused(lastFocusedRowId, item)
                                onDisableTarget?.(item)
                            }}
                        >
                            <BanIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionDisable}
                        </Button>
                    </DisabledActionHint>
                </div>
            )
        },
    }
}
