/** W12 供应商往来 · 进项发票列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"
import type { Dispatch, SetStateAction } from "react"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    PurchaseInvoiceRow,
    ReverseTarget,
    SessionState,
} from "@/features/supplier-payables/types"

export function buildInvoiceColumns(input: {
    openSession: (next: SessionState) => void
    setReverseTarget: Dispatch<SetStateAction<ReverseTarget | null>>
    setRedInvoiceNo: Dispatch<SetStateAction<string>>
}): ColumnDef<PurchaseInvoiceRow>[] {
    const { openSession, setReverseTarget, setRedInvoiceNo } = input
    return [
        {
            id: "doc",
            header: "进项发票",
            meta: { label: "发票", width: "reference" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="font-medium">
                        <span className="num">
                            {row.original.invoiceCode}-
                            {row.original.invoiceNo}
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
                    <MoneyValue value={row.original.grossAmount} taxBasis="gross" />
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
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => (
                <div className="flex flex-wrap justify-end gap-1">
                    {row.original.allowedActions.includes(
                        "CONTINUE_ALLOCATE",
                    ) ? (
                        <Button
                            type="button"
                            size="xs"
                            onClick={() =>
                                openSession({
                                    track: "purchase_invoice",
                                    supplierId: row.original.supplierId,
                                    existingInvoiceId: row.original.invoiceId,
                                })
                            }
                        >
                            继续核销
                        </Button>
                    ) : null}
                    {row.original.allowedActions.includes("RED_INVOICE") ? (
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() => {
                                setRedInvoiceNo(`R${row.original.invoiceNo}`)
                                setReverseTarget({
                                    kind: "invoice",
                                    id: row.original.invoiceId,
                                    no: `${row.original.invoiceCode}-${row.original.invoiceNo}`,
                                })
                            }}
                        >
                            红票
                        </Button>
                    ) : null}
                </div>
            ),
        },
    ]
}
