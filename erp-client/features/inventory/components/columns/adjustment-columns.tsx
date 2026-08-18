"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import type { StockAdjustmentRow } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

export function buildAdjustmentColumns(): ColumnDef<StockAdjustmentRow>[] {
    return [
        {
            id: "doc",
            header: "调整单",
            meta: { label: "调整单", width: "reference" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="num font-medium">
                        {row.original.adjustmentNo}
                    </div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.reasonTypeLabel} ·{" "}
                        {row.original.direction === "increase"
                            ? "增加"
                            : "减少"}{" "}
                        {row.original.quantity} {row.original.baseUnit}
                    </div>
                </div>
            ),
        },
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "default" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div>{row.original.warehouseName}</div>
                    <div className="num text-xs text-muted-foreground">
                        {row.original.skuCode} · {row.original.skuName}
                    </div>
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
            id: "people",
            header: "审批进度",
            meta: { label: "审批进度", width: "default" },
            cell: ({ row }) => (
                <div className="text-xs text-muted-foreground">
                    <div>经办 {row.original.operatorLabel}</div>
                    <div>当前节点 {row.original.currentNodeLabel ?? "—"}</div>
                    <div>
                        当前审批人 {row.original.currentAssigneeLabel ?? "—"}
                    </div>
                </div>
            ),
        },
        {
            id: "time",
            header: "创建 / 确认入账",
            meta: { label: "时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <div className="num text-xs text-muted-foreground">
                    <div>
                        创建{" "}
                        {formatDateTime(
                            row.original.createdAt,
                            "full",
                            "passthrough",
                        )}
                    </div>
                    <div>
                        确认入账{" "}
                        {row.original.postedAt
                            ? formatDateTime(
                                  row.original.postedAt,
                                  "full",
                                  "passthrough",
                              )
                            : "—"}
                    </div>
                </div>
            ),
        },
    ]
}
