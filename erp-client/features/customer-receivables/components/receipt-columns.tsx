import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import type { ReceiptRow } from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"
import { allocationSummaryCell } from "./allocation-summary"

export function createReceiptColumns(): ColumnDef<ReceiptRow>[] {
    return [
        {
            id: "doc",
            header: "回款单号",
            meta: { label: "回款单号", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <TableTextCell value={row.original.receiptNo} numeric />
                    }
                    primaryTitle={row.original.receiptNo}
                    secondary={row.original.counterpartyPartyName}
                    secondaryTitle={row.original.counterpartyPartyName}
                />
            ),
        },
        {
            id: "receivedAt",
            header: "到账时间",
            meta: { width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={
                        row.original.receivedAt
                            ? formatDateTime(
                                  row.original.receivedAt,
                                  "full",
                                  "passthrough",
                              )
                            : undefined
                    }
                    numeric
                />
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
                <MoneyValue
                    display="exact"
                    value={row.original.amount}
                    taxBasis="gross"
                />
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
            cell: ({ row }) => allocationSummaryCell(row.original),
        },
        {
            id: "status",
            header: "状态",
            meta: { width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                    appearance={
                        row.original.status === "posted" ? "quiet" : "badge"
                    }
                />
            ),
        },
    ]
}
