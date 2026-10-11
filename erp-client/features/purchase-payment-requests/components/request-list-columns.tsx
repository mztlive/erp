import type { ColumnDef } from "@tanstack/react-table"
import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"

export const requestColumns: ColumnDef<PaymentRequest>[] = [
    {
        id: "number",
        accessorKey: "request_no",
        header: "申请单号",
        meta: { label: "申请单号", width: "reference" },
        cell: ({ row }) => (
            <span className="num">{row.original.request_no}</span>
        ),
    },
    {
        id: "supplier",
        accessorKey: "supplier_name",
        header: "供应商 / 我方主体",
        meta: { label: "供应商 / 我方主体", width: "flex" },
        cell: ({ row }) => (
            <div>
                <p className="font-medium">{row.original.supplier_name}</p>
                <p className="text-xs text-muted-foreground">
                    {row.original.our_company_name}
                </p>
            </div>
        ),
    },
    {
        id: "applicant",
        accessorKey: "applicant_name",
        header: "申请人",
        meta: { label: "申请人", width: "default" },
    },
    {
        id: "amount",
        accessorKey: "total_amount",
        header: "申请金额",
        meta: {
            label: "申请金额",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.total_amount} />,
    },
    {
        id: "paid",
        accessorKey: "paid_amount",
        header: "有效已付",
        meta: {
            label: "有效已付",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.paid_amount} />,
    },
    {
        id: "unpaid",
        accessorKey: "unpaid_amount",
        header: "剩余待付",
        meta: {
            label: "剩余待付",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.unpaid_amount} />,
    },
    {
        id: "date",
        accessorKey: "desired_payment_date",
        header: "希望付款日",
        meta: { label: "希望付款日", width: "reference", numeric: true },
    },
    {
        id: "status",
        accessorKey: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
            <BusinessStatusBadge
                label={requestStatusLabel[row.original.status]}
                tone={requestStatusTone[row.original.status]}
            />
        ),
    },
]
