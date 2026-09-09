/** W12 供应商往来 · 进项发票列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { PurchaseInvoiceRow } from "@/features/supplier-payables/types"

export function buildInvoiceColumns(): ColumnDef<PurchaseInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "进项发票",
            meta: { label: "发票", width: "reference" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="font-medium">
                        <span className="num">
                            {row.original.invoiceCode}-{row.original.invoiceNo}
                        </span>
                        <Badge variant="neutral" className="ml-2">
                            {row.original.invoiceKindLabel}
                        </Badge>
                    </div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.supplierName}
                    </div>
                </div>
            ),
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
                <div className="text-end text-sm">
                    <MoneyValue
                        value={row.original.grossAmount}
                        taxBasis="gross"
                    />
                    <div className="text-xs text-muted-foreground">
                        未分配{" "}
                        <MoneyValue value={row.original.unallocatedAmount} />
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
                <div className="text-end">
                    <MoneyValue value={row.original.allocatedTotal} />
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
                    description="与付款进度独立"
                />
            ),
        },
    ]
}
