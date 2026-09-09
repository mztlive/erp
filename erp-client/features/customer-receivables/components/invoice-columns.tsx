import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    taxAmountToneClass,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"

/** 销项发票列表只展示事实，操作由行预览 Sheet 承载。 */
export function createInvoiceColumns(): ColumnDef<SalesInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "发票",
            meta: { label: "发票", width: "reference" },
            cell: ({ row }) => (
                <div>
                    <div className="flex items-center gap-2">
                        <span className="num text-sm font-medium">
                            {row.original.invoiceNo}
                        </span>
                        <Badge
                            variant={
                                row.original.invoiceKind === "red"
                                    ? "warning"
                                    : "secondary"
                            }
                        >
                            {row.original.invoiceKindLabel}
                        </Badge>
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.invoiceCode
                            ? `代码 ${row.original.invoiceCode} · `
                            : ""}
                        {row.original.counterpartyPartyName}
                    </div>
                </div>
            ),
        },
        {
            id: "date",
            header: "开票日期",
            cell: ({ row }) => (
                <span className="num text-sm">{row.original.invoiceDate}</span>
            ),
        },
        {
            id: "gross",
            header: "含税金额",
            meta: {
                label: "含税",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.grossAmount}
                    taxBasis="gross"
                    className={taxAmountToneClass("含税金额")}
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
