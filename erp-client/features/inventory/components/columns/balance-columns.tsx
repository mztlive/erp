"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { LoaderCircleIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    TableRowActions,
    type TableRowAction,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { formatQty } from "@/features/inventory/components/presentation"
import type { StockBalanceRow } from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

export type BalanceColumnsInput = {
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openDetail: (balanceId: string) => void
    startAdjustment: (row: StockBalanceRow) => Promise<void>
    isCreating?: boolean
}

export function buildBalanceColumns({
    rowFocusRef,
    openDetail,
    startAdjustment,
    isCreating = false,
}: BalanceColumnsInput): ColumnDef<StockBalanceRow>[] {
    return [
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="truncate text-sm font-medium">
                        {row.original.warehouseName}
                        <span className="ml-1 num text-xs text-muted-foreground">
                            {row.original.warehouseCode}
                        </span>
                    </div>
                    <div className="truncate text-sm">
                        <span className="num">{row.original.skuCode}</span>
                        <span className="text-muted-foreground"> · </span>
                        {row.original.skuName}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.specSummary}
                    </div>
                </div>
            ),
        },
        {
            id: "onHand",
            header: "账面现存",
            meta: {
                label: "账面现存",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                formatQty(row.original.onHandQuantity, row.original.baseUnit),
        },
        {
            id: "reserved",
            header: "有效预占",
            meta: {
                label: "有效预占",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                formatQty(row.original.reservedQuantity, row.original.baseUnit),
        },
        {
            id: "available",
            header: "可用数量",
            meta: {
                label: "可用数量",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-0.5">
                    {formatQty(
                        row.original.availableQuantity,
                        row.original.baseUnit,
                    )}
                    {row.original.availableQuantity === "0" ? (
                        <Badge variant="destructive" className="text-2xs">
                            零可用
                        </Badge>
                    ) : null}
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                />
            ),
        },
        {
            id: "lastMovement",
            header: "最后变动",
            meta: { label: "最后变动", width: "default" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div>{row.original.lastMovementTypeLabel}</div>
                    <div className="num text-xs text-muted-foreground">
                        {formatDateTime(
                            row.original.lastMovementAt,
                            "full",
                            "passthrough",
                        )}
                    </div>
                </div>
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => {
                const segment = toAutomationIdSegment(row.original.balanceId)
                const hasCreateAction =
                    row.original.allowedActions.includes("CREATE_ADJUSTMENT")
                const blockerMessage = row.original.actionBlockers.find(
                    (blocker) => blocker.action === "CREATE_ADJUSTMENT",
                )?.message
                const shownProduct =
                    row.original.skuName.trim() || row.original.skuCode.trim()
                const actions: TableRowAction[] = [
                    {
                        id: `inventory-ledger-balance-row-${segment}-view`,
                        label: "查看",
                        buttonRef: (el) => {
                            rowFocusRef.current.set(row.original.balanceId, el)
                        },
                        onClick: () => openDetail(row.original.balanceId),
                    },
                ]
                if (hasCreateAction) {
                    actions.push({
                        id: `inventory-ledger-balance-row-${segment}-adjust`,
                        label: isCreating ? "创建中…" : "库存调整",
                        emphasis: "outline",
                        disabled: isCreating,
                        onClick: () => {
                            void startAdjustment(row.original)
                        },
                        ...(blockerMessage
                            ? { disabledReason: blockerMessage }
                            : {}),
                        ...(isCreating
                            ? {
                                  leading: (
                                      <LoaderCircleIcon
                                          data-icon="inline-start"
                                          aria-hidden="true"
                                          className="animate-spin"
                                      />
                                  ),
                              }
                            : {}),
                    })
                }
                return (
                    <TableRowActions
                        actions={actions}
                        moreId={`inventory-ledger-balance-row-${segment}-more`}
                        moreLabel={
                            shownProduct
                                ? `${shownProduct} 更多操作`
                                : "库存 更多操作"
                        }
                    />
                )
            },
        },
    ]
}
