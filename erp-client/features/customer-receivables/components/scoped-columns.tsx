"use client"

import type { ReactNode } from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    LimitedBadge,
    MoneyValue,
} from "@/components/business"
import type {
    ScopedCustomerReceiptWire,
    ScopedInvoiceWire,
    ScopedReceivableAccountWire,
} from "@/features/customer-receivables/api/scoped"
import { formatDateTime } from "@/lib/datetime"
import { instantToIso } from "@/features/customer-receivables/api/mappers"
import { scopeText } from "@/lib/ui-text"

function restrictedReason(limited: boolean): ReactNode {
    return limited ? (
        <span className="text-xs text-muted-foreground">
            {scopeText.limitedOnlyVisibleShare}
        </span>
    ) : null
}

/** 范围应收子账列：获授权份额常显，整单金额受限时为空并注明。 */
export function createScopedReceivableColumns(): ColumnDef<ScopedReceivableAccountWire>[] {
    return [
        {
            id: "order",
            header: "销售单 / 子账",
            meta: { label: "销售单", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 flex-col items-start gap-1 whitespace-normal">
                    <span className="num break-words text-sm font-medium">
                        {row.original.sales_order_id}
                    </span>
                    <span className="text-xs text-muted-foreground">
                        子账 #{row.original.account_seq}
                    </span>
                </div>
            ),
        },
        {
            id: "visible",
            header: "获授权已核销",
            meta: {
                label: "获授权份额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue value={row.original.visible_settled_share} />
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
                        unavailableReason={
                            row.original.permission_limited
                                ? scopeText.wholeRestricted
                                : undefined
                        }
                    />
                    {restrictedReason(row.original.permission_limited)}
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
                    label={row.original.status}
                    tone="neutral"
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
            header: "回款单号",
            meta: { label: "回款单号", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="num break-words text-sm font-medium">
                        {row.original.receipt_no}
                    </div>
                    <div className="num text-xs text-muted-foreground">
                        {formatDateTime(instantToIso(row.original.received_at))}
                    </div>
                </div>
            ),
        },
        {
            id: "visible",
            header: "获授权已分配",
            meta: {
                label: "获授权份额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue value={row.original.visible_allocated_share} />
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
            cell: ({ row }) => (
                <div className="text-right">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            未分配
                        </span>
                        <MoneyValue
                            value={row.original.unallocated_amount}
                            unavailableReason={
                                row.original.permission_limited
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </div>
                    <div className="mt-1">
                        <span className="mr-2 text-xs text-muted-foreground">
                            整单
                        </span>
                        <MoneyValue
                            value={row.original.amount}
                            unavailableReason={
                                row.original.permission_limited
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.status}
                    tone="neutral"
                />
            ),
        },
    ]
}

/** 范围销项发票列：获授权分配份额常显，整单与未分配受限时为空。 */
export function createScopedInvoiceColumns(): ColumnDef<ScopedInvoiceWire>[] {
    return [
        {
            id: "doc",
            header: "发票",
            meta: { label: "发票", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="num break-words text-sm font-medium">
                        {row.original.invoice_no}
                    </div>
                    <div className="num text-xs text-muted-foreground">
                        {row.original.invoice_date}
                    </div>
                </div>
            ),
        },
        {
            id: "visible",
            header: "获授权已分配",
            meta: {
                label: "获授权份额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue value={row.original.visible_allocated_share} />
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
            cell: ({ row }) => (
                <div className="text-right">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            未分配
                        </span>
                        <MoneyValue
                            value={row.original.unallocated_amount}
                            unavailableReason={
                                row.original.permission_limited
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </div>
                    <div className="mt-1">
                        <span className="mr-2 text-xs text-muted-foreground">
                            整单
                        </span>
                        <MoneyValue
                            value={row.original.gross_amount}
                            unavailableReason={
                                row.original.permission_limited
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.status}
                    tone="neutral"
                />
            ),
        },
    ]
}
