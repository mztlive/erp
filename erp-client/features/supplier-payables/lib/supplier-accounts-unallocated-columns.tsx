/** W12 供应商往来 · 待核销列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    SessionState,
    SupplierAccountsListView,
    UnallocatedRow,
} from "@/features/supplier-payables/types"

export function buildUnallocatedColumns(input: {
    data: SupplierAccountsListView | undefined
    openSession: (next: SessionState) => void
}): ColumnDef<UnallocatedRow>[] {
    const { data, openSession } = input
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
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => {
                const payment = data?.payments.find(
                    (p) => p.paymentNo === row.original.documentNo,
                )
                const invoice = data?.invoices.find(
                    (p) =>
                        `${p.invoiceCode}-${p.invoiceNo}` ===
                        row.original.documentNo,
                )
                const resolved =
                    row.original.track === "payment" ? payment : invoice
                return (
                    <Button
                        type="button"
                        size="xs"
                        disabled={!resolved}
                        title={
                            resolved
                                ? undefined
                                : "未找到原付款/发票，请回到对应视图操作"
                        }
                        onClick={() =>
                            openSession({
                                track: row.original.track,
                                supplierId: row.original.supplierId,
                                existingPaymentId:
                                    row.original.track === "payment"
                                        ? payment?.paymentId
                                        : undefined,
                                existingInvoiceId:
                                    row.original.track ===
                                    "purchase_invoice"
                                        ? invoice?.invoiceId
                                        : undefined,
                            })
                        }
                    >
                        继续核销
                    </Button>
                )
            },
        },
    ]
}
