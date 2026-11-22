"use client"

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    LimitedBadge,
    MoneyValue,
    TableIdentityCell,
    wholeUnallocatedCell,
} from "@/components/business"
import type {
    ScopedCustomerReceiptWire,
    ScopedInvoiceWire,
    ScopedReceivableAccountWire,
} from "@/features/customer-receivables/api/scoped"
import { formatDateTime } from "@/lib/datetime"
import { instantToIso } from "@/features/customer-receivables/api/mappers"
import {
    customerReceiptStatusLabel,
    customerReceiptStatusTone,
} from "@/features/customer-receivables/lib/customer-receipt-approval"
import { RECEIVABLE_STATUS_LABEL } from "@/features/customer-receivables/types"
import { scopeText } from "@/lib/ui-text"
import {
    businessLabelOrPlaceholder,
    MISSING_SALES_ORDER_NO,
} from "../lib/display-labels"

export function scopedReceivableOrderNo(row: ScopedReceivableAccountWire) {
    return businessLabelOrPlaceholder(
        row.sales_order_no,
        row.sales_order_id,
        MISSING_SALES_ORDER_NO,
    )
}

function receivableStatus(status: string) {
    switch (status) {
        case "open":
            return {
                label: RECEIVABLE_STATUS_LABEL.open,
                tone: "info",
            } as const
        case "partially_settled":
        case "partial":
            return {
                label: RECEIVABLE_STATUS_LABEL.partial,
                tone: "warning",
            } as const
        case "settled":
            return {
                label: RECEIVABLE_STATUS_LABEL.settled,
                tone: "success",
                appearance: "quiet",
            } as const
        default:
            return { label: "状态未知", tone: "neutral" } as const
    }
}

function invoiceStatus(status: string) {
    switch (status) {
        case "draft":
            return { label: "草稿", tone: "neutral" } as const
        case "registered":
            return {
                label: "已登记",
                tone: "success",
                appearance: "quiet",
            } as const
        case "red_invoiced":
            return { label: "已红冲", tone: "void" } as const
        case "reversed":
            return { label: "已作废", tone: "void" } as const
        default:
            return { label: "状态未知", tone: "neutral" } as const
    }
}

/** 范围应收子账列：获授权份额常显，整单金额受限时为空并注明。 */
export function createScopedReceivableColumns(): ColumnDef<ScopedReceivableAccountWire>[] {
    return [
        {
            id: "order",
            header: "结算主体 / 销售单",
            meta: { label: "结算主体 / 销售单", width: "flex" },
            cell: ({ row }) => {
                const orderNo = scopedReceivableOrderNo(row.original)
                const partyName =
                    row.original.counterparty_party_name?.trim() ||
                    row.original.customer_name?.trim()
                const secondary = partyName
                    ? `${orderNo} · 应收 #${row.original.account_seq}`
                    : `应收款项 #${row.original.account_seq}`
                return (
                    <TableIdentityCell
                        primary={
                            partyName ? (
                                partyName
                            ) : (
                                <span className="num">{orderNo}</span>
                            )
                        }
                        primaryTitle={partyName || orderNo}
                        secondary={secondary}
                        secondaryTitle={secondary}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内已核销",
            meta: {
                label: "本人权限内金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue
                        value={row.original.visible_settled_share}
                        display="exact"
                        className="font-semibold"
                    />
                    {row.original.permission_limited ? (
                        <LimitedBadge
                            id={`customer-receivables-scoped-receivable-limited-${row.original.id}`}
                        />
                    ) : null}
                </div>
            ),
        },
        {
            id: "whole",
            header: "整单金额",
            meta: {
                label: "整单",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue
                        value={row.original.gross_total}
                        display="exact"
                        unavailableReason={
                            row.original.permission_limited
                                ? scopeText.wholeRestricted
                                : undefined
                        }
                    />
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
                    {...receivableStatus(row.original.status)}
                />
            ),
        },
    ]
}

/** 范围回款列：获授权分配份额常显，整单与未分配受限时为空。 */
export function createScopedReceiptColumns(): ColumnDef<ScopedCustomerReceiptWire>[] {
    return [
        {
            id: "doc",
            header: "回款单号 / 结算主体",
            meta: { label: "回款单号", width: "flex" },
            cell: ({ row }) => {
                const { receipt_no, received_at } = row.original
                const receivedAt = formatDateTime(instantToIso(received_at))
                const partyName =
                    row.original.counterparty_party_name?.trim() ||
                    row.original.customer_name?.trim()
                const secondary = partyName
                    ? `${partyName} · ${receivedAt}`
                    : receivedAt
                return (
                    <TableIdentityCell
                        primary={<span className="num">{receipt_no}</span>}
                        primaryTitle={receipt_no}
                        secondary={secondary}
                        secondaryTitle={secondary}
                        numericSecondary={!partyName}
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内已分配",
            meta: {
                label: "本人权限内金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue
                        value={row.original.visible_allocated_share}
                        display="exact"
                        className="font-semibold"
                    />
                    {row.original.permission_limited ? (
                        <LimitedBadge
                            id={`customer-receivables-scoped-receipt-limited-${row.original.id}`}
                        />
                    ) : null}
                </div>
            ),
        },
        {
            id: "whole",
            header: "整单 / 未分配",
            meta: {
                label: "整单",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                wholeUnallocatedCell({
                    unallocated: row.original.unallocated_amount,
                    whole: row.original.amount,
                    permissionLimited: row.original.permission_limited,
                }),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => {
                const status = row.original.status
                const tone = customerReceiptStatusTone(status)
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={customerReceiptStatusLabel(status)}
                        tone={tone}
                        appearance={tone === "success" ? "quiet" : "badge"}
                    />
                )
            },
        },
    ]
}

/** 范围销项发票列：获授权分配份额常显，整单与未分配受限时为空。 */
export function createScopedInvoiceColumns(): ColumnDef<ScopedInvoiceWire>[] {
    return [
        {
            id: "doc",
            header: "发票 / 往来主体",
            meta: { label: "发票", width: "flex" },
            cell: ({ row }) => {
                const { invoice_no, invoice_date, party_name } = row.original
                const title = invoice_no ?? "附件登记"
                const party = party_name?.trim()
                const secondary = [party, invoice_date]
                    .filter(Boolean)
                    .join(" · ")
                return (
                    <TableIdentityCell
                        primary={<span className="num">{title}</span>}
                        primaryTitle={title}
                        secondary={secondary || undefined}
                        secondaryTitle={secondary || undefined}
                        numericSecondary={!party}
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内已分配",
            meta: {
                label: "本人权限内金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue
                        value={row.original.visible_allocated_share}
                        display="exact"
                        className="font-semibold"
                    />
                    {row.original.permission_limited ? (
                        <LimitedBadge
                            id={`customer-receivables-scoped-invoice-limited-${row.original.id}`}
                        />
                    ) : null}
                </div>
            ),
        },
        {
            id: "whole",
            header: "整单 / 未分配",
            meta: {
                label: "整单",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                wholeUnallocatedCell({
                    unallocated: row.original.unallocated_amount,
                    whole: row.original.gross_amount,
                    permissionLimited: row.original.permission_limited,
                }),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    {...invoiceStatus(row.original.status)}
                />
            ),
        },
    ]
}
