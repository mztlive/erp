/** W12 供应商往来 · 应付台账列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    type TableRowAction,
} from "@/components/business"
import type { PayableRow } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export function buildPayableColumns(input: {
    canRegisterInvoice: boolean
    onOpenPreview: (payableAccountId: string) => void
    onRegisterInvoice: (row: PayableRow) => void
}): ColumnDef<PayableRow>[] {
    const { canRegisterInvoice, onOpenPreview, onRegisterInvoice } = input
    return [
        {
            id: "supplier",
            header: "供应商 / 来源单据",
            meta: { label: "供应商 / 来源单据", width: "flex" },
            accessorKey: "supplierName",
            cell: ({ row }) => {
                const { supplierName, sourceTypeLabel, sourceDocumentNo } =
                    row.original
                const source = `${sourceTypeLabel} · ${sourceDocumentNo}`
                return (
                    <TableIdentityCell
                        primary={supplierName}
                        primaryTitle={supplierName}
                        secondary={source}
                        secondaryTitle={source}
                        numericSecondary
                    />
                )
            },
        },
        {
            accessorKey: "openTotal",
            header: "剩余应付（含税）",
            meta: {
                label: "剩余应付（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.openTotal} display="exact" />
            ),
        },
        {
            accessorKey: "grossTotal",
            header: "应付总额（含税）",
            meta: {
                label: "应付总额（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.grossTotal} display="exact" />
            ),
        },
        {
            accessorKey: "settledTotal",
            header: "已付净额",
            meta: {
                label: "已付净额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.settledTotal} display="exact" />
            ),
        },
        {
            accessorKey: "invoicedTotal",
            header: "已收票净额",
            meta: {
                label: "已收票净额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.invoicedTotal}
                    display="exact"
                />
            ),
        },
        {
            id: "due",
            header: "到期情况",
            meta: { label: "到期情况", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-0.5">
                    <TableTextCell value={row.original.dueDate} numeric />
                    <span
                        className={cn(
                            "num text-xs",
                            row.original.dueState === "overdue"
                                ? "text-destructive"
                                : "text-muted-foreground",
                        )}
                    >
                        {row.original.dueStateLabel}
                    </span>
                </div>
            ),
        },
        {
            id: "status",
            header: "结清状态",
            meta: { label: "结清状态", width: "status" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1">
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                        appearance={
                            row.original.status === "SETTLED"
                                ? "quiet"
                                : "badge"
                        }
                    />
                    {row.original.paymentGateSummary &&
                    row.original.paymentGateSummary.state !==
                        "NOT_APPLICABLE" ? (
                        <span
                            className={cn(
                                "text-xs",
                                row.original.paymentGateSummary.state ===
                                    "BLOCKED"
                                    ? "text-destructive"
                                    : "text-muted-foreground",
                            )}
                        >
                            先款条件
                            {row.original.paymentGateSummary.state ===
                            "SATISFIED"
                                ? "已满足"
                                : "未满足"}
                        </span>
                    ) : null}
                </div>
            ),
        },
        {
            id: "actions",
            header: () => <span className="sr-only">操作</span>,
            meta: {
                label: "操作",
                width: "status",
                align: "end",
            },
            cell: ({ row }) => {
                const payable = row.original
                const prefix = `supplier-payables-table-payable-row-${toAutomationIdSegment(payable.payableAccountId)}`
                const canInvoice =
                    canRegisterInvoice &&
                    payable.allowedActions.includes("REGISTER_INVOICE")
                const actions: TableRowAction[] = [
                    {
                        id: `${prefix}-request-payment`,
                        label: "申请付款",
                        hidden: payable.sourceType !== "PURCHASE_ORDER",
                        href: `/procurement/payment-requests?${new URLSearchParams({ purchaseOrderId: payable.sourceDocumentId, create: "1" })}`,
                    },
                    {
                        id: `${prefix}-register-invoice`,
                        label: "登记进项票",
                        hidden: !canInvoice,
                        placement:
                            payable.sourceType === "PURCHASE_ORDER"
                                ? "menu"
                                : "inline",
                        onClick: () => onRegisterInvoice(payable),
                    },
                    {
                        id: `${prefix}-detail`,
                        label: "详情",
                        onClick: () => onOpenPreview(payable.payableAccountId),
                    },
                ]
                return (
                    <TableRowActions
                        moreId={`${prefix}-more`}
                        moreLabel={`${payable.supplierName} ${payable.sourceDocumentNo} 更多操作`}
                        actions={actions}
                    />
                )
            },
        },
    ]
}
