"use client"

import type { ColumnDef } from "@tanstack/react-table"

import {
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { WarehouseSkuIdentity } from "@/features/inventory/components/columns/warehouse-sku-cell"
import type { StockAdjustmentRow } from "@/features/inventory/types"
import { inventoryStatusColumn } from "@/features/inventory/components/columns/status-column"
import { formatDateTime } from "@/lib/datetime"

export function buildAdjustmentColumns(): ColumnDef<StockAdjustmentRow>[] {
    return [
        {
            id: "doc",
            header: "调整单",
            meta: { label: "调整单", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.adjustmentNo}
                    primaryTitle={row.original.adjustmentNo}
                    secondary={
                        <>
                            {row.original.reasonTypeLabel} ·{" "}
                            {row.original.direction === "increase"
                                ? "增加"
                                : "减少"}{" "}
                            <QuantityValue
                                value={row.original.quantity}
                                unit={row.original.baseUnit}
                                className="[&>.num]:text-inherit"
                            />
                        </>
                    }
                    className="num"
                />
            ),
        },
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "reference" },
            cell: ({ row }) => (
                <WarehouseSkuIdentity
                    warehouseName={row.original.warehouseName}
                    skuCode={row.original.skuCode}
                    skuName={row.original.skuName}
                />
            ),
        },
        inventoryStatusColumn<StockAdjustmentRow>(),
        {
            id: "people",
            header: "审批进度",
            meta: { label: "审批进度", width: "default" },
            cell: ({ row }) => (
                <div className="text-xs text-muted-foreground">
                    <div>
                        经办{" "}
                        <TableTextCell value={row.original.operatorLabel} />
                    </div>
                    <div>
                        申请人{" "}
                        <TableTextCell value={row.original.applicantLabel} />
                    </div>
                    <div>
                        当前节点{" "}
                        <TableTextCell value={row.original.currentNodeLabel} />
                    </div>
                    <div>
                        当前审批人{" "}
                        <TableTextCell
                            value={row.original.currentAssigneeLabel}
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "time",
            header: "创建 / 确认入账",
            meta: { label: "创建 / 确认入账", width: "amount", numeric: true },
            cell: ({ row }) => (
                <div className="num text-muted-foreground">
                    <div>创建 {formatDateTime(row.original.createdAt)}</div>
                    <div>
                        确认入账{" "}
                        {row.original.postedAt ? (
                            formatDateTime(row.original.postedAt)
                        ) : (
                            <TableEmptyCell />
                        )}
                    </div>
                </div>
            ),
        },
    ]
}
