/** W12 供应商往来 · 付款列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PaymentRow } from "@/features/supplier-payables/types"

export function buildPaymentColumns(input: {
    openPaymentPreview: (paymentId: string) => void
    openReversalPreview: (reversalId: string, paymentId: string) => void
}): ColumnDef<PaymentRow>[] {
    const { openPaymentPreview, openReversalPreview } = input
    return [
        {
            id: "doc",
            header: "付款单",
            meta: { label: "付款单", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <span className="num">{row.original.paymentNo}</span>
                    }
                    primaryTitle={row.original.paymentNo}
                    secondary={row.original.supplierName}
                    secondaryTitle={row.original.supplierName}
                />
            ),
        },
        {
            id: "amount",
            header: "金额 / 未核销",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                row.original.permissionLimited ? (
                    <TableEmptyCell />
                ) : (
                    <div>
                        <MoneyValue
                            value={row.original.amount}
                            taxBasis="gross"
                            display="exact"
                        />
                        <div className="text-xs text-muted-foreground">
                            未核销{" "}
                            <MoneyValue
                                value={row.original.unallocatedAmount}
                                display="exact"
                            />
                        </div>
                    </div>
                ),
        },
        {
            id: "bank",
            header: "银行引用",
            meta: { label: "银行", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.bankReferenceMasked}
                    numeric
                    clamp={1}
                />
            ),
        },
        {
            id: "reversal",
            header: "关联冲正",
            meta: { label: "关联冲正", width: "default", align: "end" },
            cell: ({ row }) => {
                const [latest, ...older] = row.original.relatedReversals ?? []
                if (!latest) return <TableEmptyCell />
                const paymentId = row.original.paymentId
                const actionPrefix = `supplier-payables-table-row-${toAutomationIdSegment(paymentId)}`
                return (
                    <div className="flex flex-col items-end gap-0.5">
                        <TableRowActions
                            moreId={`${actionPrefix}-more`}
                            moreLabel={`${row.original.paymentNo} 更多操作`}
                            actions={[
                                {
                                    id: `${actionPrefix}-reversal-${toAutomationIdSegment(latest.reversalId)}-open`,
                                    label: (
                                        <span
                                            className="num block max-w-28 truncate"
                                            title={latest.reversalNo}
                                        >
                                            {latest.reversalNo}
                                        </span>
                                    ),
                                    onClick: () =>
                                        openReversalPreview(
                                            latest.reversalId,
                                            paymentId,
                                        ),
                                },
                            ]}
                        />
                        <span className="flex flex-wrap items-center justify-end gap-1">
                            <BusinessStatusBadge
                                context="list"
                                label={latest.statusLabel}
                                tone={latest.statusTone}
                            />
                            {older.length > 0 ? (
                                <span className="num text-xs text-muted-foreground">
                                    另 {older.length} 条
                                </span>
                            ) : null}
                        </span>
                    </div>
                )
            },
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
                        row.original.status === "POSTED" ? "quiet" : "badge"
                    }
                />
            ),
        },
        {
            id: "time",
            header: "付款时间",
            meta: { label: "时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <TableTextCell
                    value={
                        row.original.paidAt &&
                        formatDateTime(
                            row.original.paidAt,
                            "full",
                            "passthrough",
                        )
                    }
                    clamp={1}
                    numeric
                />
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
                const payment = row.original
                const latestReversal = payment.relatedReversals?.[0]
                const prefix = `supplier-payables-table-payment-row-${toAutomationIdSegment(payment.paymentId)}`
                return (
                    <TableRowActions
                        moreId={`${prefix}-more`}
                        moreLabel={`${payment.paymentNo} 更多操作`}
                        actions={[
                            {
                                id: `${prefix}-detail`,
                                label: "详情",
                                onClick: () =>
                                    openPaymentPreview(payment.paymentId),
                            },
                            {
                                id: `${prefix}-reversal`,
                                label: "查看冲正",
                                hidden: !latestReversal,
                                placement: "menu",
                                onClick: () => {
                                    if (latestReversal) {
                                        openReversalPreview(
                                            latestReversal.reversalId,
                                            payment.paymentId,
                                        )
                                    }
                                },
                            },
                        ]}
                    />
                )
            },
        },
    ]
}
