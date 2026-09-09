import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"

export function createReceivableColumns(): ColumnDef<ReceivableAccountRow>[] {
    return [
        {
            id: "party",
            header: "往来主体 / 客户",
            meta: { label: "往来主体", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 max-w-64 flex-col items-start gap-1 whitespace-normal">
                    <span className="break-words text-sm font-medium">
                        {row.original.counterpartyPartyName}
                    </span>
                    <span className="sr-only">·</span>
                    <span className="break-words text-xs text-muted-foreground">
                        {row.original.customerName}
                    </span>
                </div>
            ),
        },
        {
            id: "order",
            header: "销售单 / 子账",
            meta: { label: "销售单", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1">
                    <span className="num text-sm">
                        {row.original.salesOrderNo}
                    </span>
                    <span className="text-xs text-muted-foreground">
                        子账 #{row.original.accountSeq} ·{" "}
                        {row.original.businessTypeLabel}
                    </span>
                </div>
            ),
        },
        {
            id: "open",
            header: "开放应收（含税）",
            meta: {
                label: "开放应收",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => <MoneyValue value={row.original.openTotal} />,
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
            cell: ({ row }) => <MoneyValue value={row.original.settledTotal} />,
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
                        <MoneyValue value={row.original.openInvoiceableTotal} />
                    </div>
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            净已开票
                        </span>
                        <MoneyValue value={row.original.invoicedTotal} />
                    </div>
                </div>
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1.5">
                    <span className="num text-sm">{row.original.dueDate}</span>
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.dueStateLabel}
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
                <div className="flex flex-col items-start gap-1.5">
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                </div>
            ),
        },
    ]
}
