import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import type { ReceiptRow } from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"

export function createReceiptColumns(): ColumnDef<ReceiptRow>[] {
    return [
        {
            id: "doc",
            header: "回款单号",
            meta: { label: "回款单号", width: "reference" },
            cell: ({ row }) => (
                <div>
                    <div className="num text-sm font-medium">
                        {row.original.receiptNo}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.counterpartyPartyName}
                    </div>
                </div>
            ),
        },
        {
            id: "receivedAt",
            header: "到账时间",
            cell: ({ row }) => (
                <span className="num text-sm">
                    {formatDateTime(
                        row.original.receivedAt,
                        "full",
                        "passthrough",
                    )}
                </span>
            ),
        },
        {
            id: "amount",
            header: "到账金额",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.amount} taxBasis="gross" />
            ),
        },
        {
            id: "alloc",
            header: "净已分配 / 未分配",
            meta: {
                label: "分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="text-right">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            未分配
                        </span>
                        <MoneyValue value={row.original.unallocatedAmount} />
                    </div>
                    <div className="mt-1">
                        <span className="mr-2 text-xs text-muted-foreground">
                            净已分配
                        </span>
                        <MoneyValue value={row.original.allocatedTotal} />
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                />
            ),
        },
    ]
}
