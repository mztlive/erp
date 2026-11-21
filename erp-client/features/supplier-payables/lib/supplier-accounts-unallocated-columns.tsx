/** W12 供应商往来 · 待核销列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { MoneyValue, TableIdentityCell } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { UnallocatedRow } from "@/features/supplier-payables/types"

export function buildUnallocatedColumns(): ColumnDef<UnallocatedRow>[] {
    return [
        {
            id: "track",
            header: "单据类型",
            meta: { label: "单据类型", width: "default" },
            cell: ({ row }) => (
                <Badge
                    variant={
                        row.original.track === "payment" ? "warning" : "info"
                    }
                >
                    {row.original.trackLabel}
                </Badge>
            ),
        },
        {
            id: "doc",
            header: "单据 / 供应商",
            meta: { label: "单据", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <span className="num">{row.original.documentNo}</span>
                    }
                    primaryTitle={row.original.documentNo}
                    secondary={row.original.supplierName}
                    secondaryTitle={row.original.supplierName}
                />
            ),
        },
        {
            id: "amount",
            header: "未分配余额",
            meta: {
                label: "余额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div>
                    <MoneyValue
                        value={row.original.unallocatedAmount}
                        taxBasis="gross"
                        display="exact"
                    />
                    <div className="text-xs text-muted-foreground">
                        记录{" "}
                        <MoneyValue
                            value={row.original.amount}
                            display="exact"
                        />
                    </div>
                </div>
            ),
        },
    ]
}
