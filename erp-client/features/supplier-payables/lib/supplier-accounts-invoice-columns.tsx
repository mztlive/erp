/** W12 供应商往来 · 进项发票列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { PurchaseInvoiceRow } from "@/features/supplier-payables/types"

export function buildInvoiceColumns(): ColumnDef<PurchaseInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "进项发票",
            meta: { label: "发票", width: "flex" },
            cell: ({ row }) => {
                const {
                    invoiceCode,
                    invoiceNo,
                    invoiceKindLabel,
                    supplierName,
                } = row.original
                const number = invoiceCode
                    ? `${invoiceCode}-${invoiceNo}`
                    : invoiceNo
                return (
                    <div className="flex min-w-0 items-start gap-2">
                        <TableIdentityCell
                            primary={<span className="num">{number}</span>}
                            primaryTitle={number}
                            secondary={supplierName}
                            secondaryTitle={supplierName}
                        />
                        <Badge variant="neutral">{invoiceKindLabel}</Badge>
                    </div>
                )
            },
        },
        {
            id: "amount",
            header: "含税 / 未分配",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div>
                    <MoneyValue
                        value={row.original.grossAmount}
                        taxBasis="gross"
                        display="exact"
                    />
                    <div className="text-xs text-muted-foreground">
                        未分配{" "}
                        <MoneyValue
                            value={row.original.unallocatedAmount}
                            display="exact"
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "alloc",
            header: "净已分配",
            meta: {
                label: "分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.allocatedTotal}
                    display="exact"
                />
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
                        row.original.statusTone === "success"
                            ? "quiet"
                            : "badge"
                    }
                    description="与付款进度独立"
                />
            ),
        },
    ]
}
