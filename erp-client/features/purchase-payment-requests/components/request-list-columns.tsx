import type { ColumnDef, VisibilityState } from "@tanstack/react-table"
import Link from "next/link"
import {
    BusinessStatusBadge,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"

export const defaultRequestColumnOrder = [
    "supplier",
    "payer",
    "amount",
    "paid",
    "unpaid",
    "status",
    "applicant",
    "date",
    "number",
    "purchaseOrders",
]

export const defaultRequestColumnPinning = { left: ["supplier"] }

export const defaultRequestColumnVisibility: VisibilityState = {
    number: false,
    purchaseOrders: false,
}

export function buildRequestColumns({
    listReturnHref,
    canOpenPurchaseOrder,
}: {
    listReturnHref: string
    canOpenPurchaseOrder: boolean
}): ColumnDef<PaymentRequest>[] {
    return [
        {
            id: "number",
            accessorKey: "request_no",
            header: "申请单号",
            meta: { label: "申请单号", width: "reference" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.request_no}
                    numeric
                    clamp={1}
                />
            ),
        },
        {
            id: "purchaseOrders",
            header: "关联采购单",
            meta: { label: "关联采购单", width: "reference" },
            cell: ({ row }) => {
                const first = row.original.lines[0]
                if (!first) return <TableEmptyCell />
                return (
                    <div className="min-w-0 space-y-1">
                        {canOpenPurchaseOrder ? (
                            <Link
                                id={`purchase-payment-requests-table-row-${toAutomationIdSegment(row.original.id)}-purchase-order`}
                                href={`/procurement/orders/${encodeURIComponent(first.purchase_order_id)}?${new URLSearchParams({ section: "payable", returnTo: listReturnHref })}`}
                                className="num block truncate text-primary underline-offset-2 hover:underline"
                                title={first.purchase_order_no}
                            >
                                {first.purchase_order_no}
                            </Link>
                        ) : (
                            <TableTextCell
                                value={first.purchase_order_no}
                                numeric
                                clamp={1}
                            />
                        )}
                        {row.original.lines.length > 1 && (
                            <p className="num text-xs text-muted-foreground">
                                等 {row.original.lines.length} 张
                            </p>
                        )}
                    </div>
                )
            },
        },
        {
            id: "supplier",
            accessorKey: "supplier_name",
            header: "供应商 / 申请单号",
            meta: { label: "供应商 / 申请单号", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.supplier_name}
                    primaryTitle={row.original.supplier_name}
                    secondary={row.original.request_no}
                    secondaryTitle={row.original.request_no}
                    numericSecondary
                />
            ),
        },
        {
            id: "payer",
            accessorKey: "our_company_name",
            header: "我方主体",
            meta: { label: "我方主体", width: "default" },
            enableSorting: false,
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.our_company_name}
                    clamp={1}
                />
            ),
        },
        {
            id: "applicant",
            accessorKey: "applicant_name",
            header: "申请人",
            meta: { label: "申请人", width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.applicant_name} clamp={1} />
            ),
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
            cell: ({ row }) => (
                <MoneyValue
                    display="compact"
                    value={row.original.total_amount}
                />
            ),
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
            cell: ({ row }) => (
                <MoneyValue
                    display="compact"
                    value={row.original.paid_amount}
                />
            ),
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
            cell: ({ row }) => (
                <MoneyValue
                    display="compact"
                    value={row.original.unpaid_amount}
                />
            ),
        },
        {
            id: "date",
            accessorKey: "desired_payment_date",
            header: "希望付款日",
            meta: { label: "希望付款日", width: "default", numeric: true },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.desired_payment_date}
                    numeric
                />
            ),
        },
        {
            id: "status",
            accessorKey: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    appearance={
                        row.original.status === "paid" ? "quiet" : "badge"
                    }
                    label={requestStatusLabel[row.original.status]}
                    tone={requestStatusTone[row.original.status]}
                />
            ),
        },
    ]
}
