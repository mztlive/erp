"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatQty } from "@/features/inventory/components/presentation"
import type { StockBalanceRow } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

export type BalanceColumnsInput = {
    isPhoneNarrow: boolean
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openDetail: (balanceId: string) => void
    startAdjustment: (row: StockBalanceRow) => Promise<void>
}

export function buildBalanceColumns({
    isPhoneNarrow,
    rowFocusRef,
    openDetail,
    startAdjustment,
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
                formatQty(
                    row.original.reservedQuantity,
                    row.original.baseUnit,
                ),
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
                const canAdjust =
                    !isPhoneNarrow &&
                    row.original.allowedActions.includes("CREATE_ADJUSTMENT")
                const blocker = isPhoneNarrow
                    ? {
                          action: "CREATE_ADJUSTMENT",
                          code: "MOBILE_READONLY",
                          message: "窄屏仅只读，请在桌面发起库存调整",
                      }
                    : row.original.actionBlockers.find(
                          (b) => b.action === "CREATE_ADJUSTMENT",
                      )
                return (
                    <div className="flex justify-end gap-1">
                        <Button
                            type="button"
                            variant="ghost"
                            size="xs"
                            ref={(el) => {
                                rowFocusRef.current.set(
                                    row.original.balanceId,
                                    el,
                                )
                            }}
                            onClick={() =>
                                openDetail(row.original.balanceId)
                            }
                        >
                            查看
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={!canAdjust}
                            title={blocker?.message}
                            onClick={() =>
                                void startAdjustment(row.original)
                            }
                        >
                            库存调整
                        </Button>
                    </div>
                )
            },
        },
    ]
}
