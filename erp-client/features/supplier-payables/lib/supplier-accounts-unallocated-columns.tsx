/** W12 供应商往来 · 待核销列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { UnallocatedRow } from "@/features/supplier-payables/types"

export function buildUnallocatedColumns(): ColumnDef<UnallocatedRow>[] {
    return [
        {
            id: "track",
            header: "轨道",
            meta: { label: "轨道", width: "default" },
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
            meta: { label: "单据", width: "reference" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="num font-medium">
                        {row.original.documentNo}
                    </div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.supplierName}
                    </div>
                </div>
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
                <div className="text-end">
                    <MoneyValue
                        value={row.original.unallocatedAmount}
                        taxBasis="gross"
                    />
                    <div className="text-xs text-muted-foreground">
                        记录 <MoneyValue value={row.original.amount} />
                    </div>
                </div>
            ),
        },
    ]
}
