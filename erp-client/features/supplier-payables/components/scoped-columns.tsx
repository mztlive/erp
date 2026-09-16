"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    LimitedBadge,
    MoneyValue,
} from "@/components/business"
import type {
    ScopedPayableAccountWire,
    ScopedPurchaseInvoiceAllocationWire,
    ScopedSupplierPaymentWire,
} from "@/features/supplier-payables/api/scoped"
import { scopeText } from "@/lib/ui-text"

/** 范围应付子账列：获授权份额常显，整单金额受限时为空并注明。 */
export function createSupplierScopedPayableColumns(): ColumnDef<ScopedPayableAccountWire>[] {
    return [
        {
            id: "source",
            header: "来源单据",
            meta: { label: "来源单据", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="num break-words text-sm font-medium">
                        {row.original.source_document_id}
                    </div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.source_type}
                    </div>
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
                            id={`supplier-payables-scope-payable-limited-${row.original.id}`}
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
                            value={row.original.open_total}
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
                            value={row.original.gross_total}
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

/** 范围付款列：获授权分配份额常显，整单与未分配受限时为空。 */
export function createSupplierScopedPaymentColumns(): ColumnDef<ScopedSupplierPaymentWire>[] {
    return [
        {
            id: "doc",
            header: "付款单号",
            meta: { label: "付款单号", width: "reference" },
            cell: ({ row }) => (
                <div className="num min-w-0 break-words text-sm font-medium">
                    {row.original.payment_no}
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
                            id={`supplier-payables-scope-payment-limited-${row.original.id}`}
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

/** 范围进项发票分配列：获授权分配常显，整单分配受限时为空。 */
export function createSupplierScopedAllocationColumns(): ColumnDef<ScopedPurchaseInvoiceAllocationWire>[] {
    return [
        {
            id: "doc",
            header: "进项发票 / 应付子账",
            meta: { label: "分配", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <div className="num break-words text-sm font-medium">
                        {row.original.invoice_no ?? row.original.invoice_id}
                    </div>
                    <div className="num text-xs text-muted-foreground">
                        {row.original.payable_account_id}
                    </div>
                </div>
            ),
        },
        {
            id: "visible",
            header: "获授权分配",
            meta: {
                label: "获授权份额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex flex-col items-end gap-1 text-right">
                    <MoneyValue value={row.original.visible_allocated_amount} />
                    {row.original.permission_limited ? (
                        <LimitedBadge
                            id={`supplier-payables-scope-allocation-limited-${row.original.id}`}
                        />
                    ) : null}
                </div>
            ),
        },
        {
            id: "whole",
            header: "整单分配",
            meta: {
                label: "整单",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.allocated_gross_amount}
                    unavailableReason={
                        row.original.permission_limited
                            ? scopeText.wholeRestricted
                            : undefined
                    }
                />
            ),
        },
    ]
}
