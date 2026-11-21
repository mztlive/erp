import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"

export function createReceivableColumns(): ColumnDef<ReceivableAccountRow>[] {
    return [
        {
            id: "party",
            header: "往来主体 / 客户",
            meta: { label: "往来主体", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.counterpartyPartyName}
                    primaryTitle={row.original.counterpartyPartyName}
                    secondary={row.original.customerName}
                    secondaryTitle={row.original.customerName}
                />
            ),
        },
        {
            id: "order",
            header: "销售单 / 应收款项",
            meta: { label: "销售单", width: "default" },
            cell: ({ row }) => {
                const reference = `应收款项 #${row.original.accountSeq} · ${row.original.businessTypeLabel}`
                return (
                    <TableIdentityCell
                        primary={
                            <TableTextCell
                                value={row.original.salesOrderNo}
                                numeric
                            />
                        }
                        primaryTitle={row.original.salesOrderNo}
                        secondary={reference}
                        secondaryTitle={reference}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "open",
            header: "待收金额（含税）",
            meta: {
                label: "待收金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.openTotal} />
            ),
        },
        {
            id: "settled",
            header: "已核销回款（含税）",
            meta: {
                label: "已核销",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.settledTotal} />
            ),
        },
        {
            id: "invoice",
            header: "净已开票 / 可开票（含税）",
            meta: {
                label: "开票",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            可开
                        </span>
                        <MoneyValue
                            display="exact"
                            value={row.original.openInvoiceableTotal}
                        />
                    </div>
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            净已开票
                        </span>
                        <MoneyValue
                            display="exact"
                            value={row.original.invoicedTotal}
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1">
                    <TableTextCell value={row.original.dueDate} numeric />
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.dueStateLabel}
                        appearance={
                            row.original.dueState === "not_due" ||
                            row.original.dueState === "unknown"
                                ? "quiet"
                                : "badge"
                        }
                        tone={
                            row.original.dueState === "overdue"
                                ? "destructive"
                                : row.original.dueState === "due_today"
                                  ? "warning"
                                  : "neutral"
                        }
                    />
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
                    appearance={
                        row.original.status === "settled" ? "quiet" : "badge"
                    }
                />
            ),
        },
    ]
}
