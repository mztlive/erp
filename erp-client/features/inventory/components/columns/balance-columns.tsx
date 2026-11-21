"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { EyeIcon, SlidersHorizontalIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    type TableRowAction,
} from "@/components/business"
import { inventoryStatusColumn } from "@/features/inventory/components/columns/status-column"
import { WarehouseSkuIdentity } from "@/features/inventory/components/columns/warehouse-sku-cell"
import type { StockBalanceRow } from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { compactFixed } from "@/lib/fixed-decimal"

export type BalanceColumnsInput = {
    openDetail?: (balanceId: string) => void
    startAdjustment?: (row: StockBalanceRow) => Promise<void>
    isCreating?: boolean
}

export function buildBalanceColumns({
    openDetail,
    startAdjustment,
    isCreating = false,
}: BalanceColumnsInput): ColumnDef<StockBalanceRow>[] {
    const columns: ColumnDef<StockBalanceRow>[] = [
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "flex" },
            cell: ({ row }) => (
                <WarehouseSkuIdentity
                    warehouseName={row.original.warehouseName}
                    warehouseCode={row.original.warehouseCode}
                    skuCode={row.original.skuCode}
                    skuName={row.original.skuName}
                    specSummary={row.original.specSummary}
                />
            ),
        },
        {
            id: "onHand",
            header: "账面现存",
            meta: {
                label: "账面现存",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <QuantityValue
                    value={row.original.onHandQuantity}
                    unit={row.original.baseUnit}
                />
            ),
        },
        {
            id: "reserved",
            header: "有效预占",
            meta: {
                label: "有效预占",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <QuantityValue
                    value={row.original.reservedQuantity}
                    unit={row.original.baseUnit}
                />
            ),
        },
        {
            id: "available",
            header: "可用数量",
            meta: {
                label: "可用数量",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const { availableQuantity, baseUnit, availability } =
                    row.original
                const zero = compactFixed(availableQuantity) === "0"
                return (
                    <div className="flex flex-col items-end gap-0.5">
                        <QuantityValue
                            value={availableQuantity}
                            unit={baseUnit}
                            tone={zero ? "warning" : "neutral"}
                        />
                        {zero && availability !== "zero" ? (
                            <BusinessStatusBadge
                                context="list"
                                tone="warning"
                                label="零可用"
                            />
                        ) : null}
                    </div>
                )
            },
        },
        inventoryStatusColumn<StockBalanceRow>(),
        {
            id: "lastMovement",
            header: "最后变动",
            meta: { label: "最后变动", width: "default" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <TableTextCell
                            value={row.original.lastMovementTypeLabel}
                        />
                    }
                    secondary={
                        row.original.lastMovementAt
                            ? formatDateTime(row.original.lastMovementAt)
                            : undefined
                    }
                    numericSecondary
                />
            ),
        },
    ]

    if (openDetail || startAdjustment)
        columns.push({
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "status", align: "end" },
            cell: ({ row }) => {
                const segment = toAutomationIdSegment(row.original.balanceId)
                const hasCreateAction =
                    row.original.allowedActions.includes("CREATE_ADJUSTMENT")
                const blockerMessage = row.original.actionBlockers.find(
                    (blocker) => blocker.action === "CREATE_ADJUSTMENT",
                )?.message
                const shownProduct =
                    row.original.skuName.trim() || row.original.skuCode.trim()
                const actions: TableRowAction[] = []
                if (openDetail) {
                    actions.push({
                        id: `inventory-ledger-balance-row-${segment}-view`,
                        label: "查看",
                        icon: EyeIcon,
                        onClick: () => openDetail(row.original.balanceId),
                    })
                }
                if (startAdjustment && (hasCreateAction || blockerMessage)) {
                    actions.push({
                        id: `inventory-ledger-balance-row-${segment}-adjust`,
                        label: isCreating ? "创建中…" : "库存调整",
                        icon: SlidersHorizontalIcon,
                        disabled: isCreating || !hasCreateAction,
                        loading: isCreating,
                        onClick: () => {
                            void startAdjustment(row.original)
                        },
                        ...(blockerMessage
                            ? { disabledReason: blockerMessage }
                            : {}),
                    })
                }
                if (!actions.length) return <TableEmptyCell />
                return (
                    <TableRowActions
                        actions={actions}
                        className="max-sm:[&_[data-slot=button]]:px-1"
                        moreId={`inventory-ledger-balance-row-${segment}-more`}
                        moreLabel={
                            shownProduct
                                ? `${shownProduct} 更多操作`
                                : "库存 更多操作"
                        }
                    />
                )
            },
        })
    return columns
}
