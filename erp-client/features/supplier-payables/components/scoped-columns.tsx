"use client"

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    LimitedBadge,
    MoneyValue,
    TableIdentityCell,
    wholeUnallocatedCell,
    type BusinessStatusBadgeProps,
} from "@/components/business"
import { instantToIso } from "@/features/supplier-payables/api/mappers"
import type {
    ScopedPayableAccountWire,
    ScopedPurchaseInvoiceAllocationWire,
    ScopedSupplierPaymentWire,
} from "@/features/supplier-payables/api/scoped"
import { formatDateTime } from "@/lib/datetime"
import { scopeText } from "@/lib/ui-text"
import {
    businessLabelOrPlaceholder,
    MISSING_SOURCE_DOCUMENT_NO,
} from "../lib/display-labels"

const sourceLabel: Record<string, string> = {
    PURCHASE_ORDER: "采购单",
    SUPPLIER_SETTLEMENT: "供应商结算单",
    SUPPLIER_SETTLEMENT_STATEMENT: "供应商结算单",
}

const statuses: Record<
    string,
    Pick<BusinessStatusBadgeProps, "label" | "tone" | "appearance">
> = {
    OPEN: { label: "未结", tone: "neutral", appearance: "quiet" },
    PARTIAL: { label: "部分结清", tone: "warning" },
    PARTIALLY_SETTLED: { label: "部分结清", tone: "warning" },
    SETTLED: { label: "已结清", tone: "success", appearance: "quiet" },
    DRAFT: { label: "草稿", tone: "neutral", appearance: "quiet" },
    POSTED: { label: "已过账", tone: "success", appearance: "quiet" },
    REVERSED: { label: "已冲正", tone: "void" },
}

function ScopeStatusCell({ status }: { status: string }) {
    return (
        <BusinessStatusBadge
            context="list"
            {...(statuses[status.toUpperCase()] ?? {
                label: "状态待确认",
                tone: "warning",
            })}
        />
    )
}

function ScopeShareCell({
    value,
    limitedId,
}: {
    value: string
    limitedId?: string
}) {
    return (
        <div className="flex flex-col items-end gap-1">
            <MoneyValue
                value={value}
                display="exact"
                className="font-semibold"
            />
            {limitedId ? <LimitedBadge id={limitedId} /> : null}
        </div>
    )
}

export function payableSourceNumber(row: ScopedPayableAccountWire) {
    return businessLabelOrPlaceholder(
        row.source_document_no,
        row.source_document_id,
        MISSING_SOURCE_DOCUMENT_NO,
    )
}

export function allocationNumber(row: ScopedPurchaseInvoiceAllocationWire) {
    return businessLabelOrPlaceholder(
        row.invoice_no,
        row.invoice_id,
        "发票号码待补全",
    )
}

export function allocationPayableNumber(
    row: ScopedPurchaseInvoiceAllocationWire,
) {
    return businessLabelOrPlaceholder(
        row.payable_no || row.source_document_no,
        row.payable_account_id,
        "应付编号待补全",
    )
}

/** 范围应付子账列：获授权份额常显，整单金额受限时为空并注明。 */
export function createSupplierScopedPayableColumns(): ColumnDef<ScopedPayableAccountWire>[] {
    return [
        {
            id: "source",
            header: "供应商 / 来源单据",
            meta: { label: "供应商 / 来源单据", width: "flex" },
            cell: ({ row }) => {
                const number = payableSourceNumber(row.original)
                const supplier = row.original.supplier_name?.trim()
                const sourceKind =
                    sourceLabel[row.original.source_type.toUpperCase()] ??
                    "来源单据"
                const secondary = supplier
                    ? `${sourceKind} · ${number}`
                    : sourceKind
                return (
                    <TableIdentityCell
                        primary={
                            supplier ? (
                                supplier
                            ) : (
                                <span className="num">{number}</span>
                            )
                        }
                        primaryTitle={supplier || number}
                        secondary={secondary}
                        secondaryTitle={secondary}
                        numericSecondary={Boolean(supplier)}
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内已核销",
            meta: {
                label: "权限内已核销",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <ScopeShareCell
                    value={row.original.visible_settled_share}
                    limitedId={
                        row.original.permission_limited
                            ? `supplier-payables-scope-payable-limited-${row.original.id}`
                            : undefined
                    }
                />
            ),
        },
        {
            id: "whole",
            header: "整单 / 未分配",
            meta: {
                label: "整单 / 未分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                wholeUnallocatedCell({
                    unallocated: row.original.open_total,
                    whole: row.original.gross_total,
                    permissionLimited: row.original.permission_limited,
                }),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => <ScopeStatusCell status={row.original.status} />,
        },
    ]
}

/** 范围付款列：获授权分配份额常显，整单与未分配受限时为空。 */
export function createSupplierScopedPaymentColumns(): ColumnDef<ScopedSupplierPaymentWire>[] {
    return [
        {
            id: "doc",
            header: "付款单号 / 供应商",
            meta: { label: "付款单号", width: "flex" },
            cell: ({ row }) => {
                const supplier = row.original.supplier_name?.trim()
                const paidAt = row.original.paid_at
                    ? formatDateTime(instantToIso(row.original.paid_at))
                    : ""
                const secondary = [supplier, paidAt].filter(Boolean).join(" · ")
                return (
                    <TableIdentityCell
                        primary={
                            <span className="num">
                                {row.original.payment_no}
                            </span>
                        }
                        primaryTitle={row.original.payment_no}
                        secondary={secondary || undefined}
                        secondaryTitle={secondary || undefined}
                        numericSecondary={!supplier}
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内已分配",
            meta: {
                label: "权限内已分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <ScopeShareCell
                    value={row.original.visible_allocated_share}
                    limitedId={
                        row.original.permission_limited
                            ? `supplier-payables-scope-payment-limited-${row.original.id}`
                            : undefined
                    }
                />
            ),
        },
        {
            id: "whole",
            header: "整单 / 未分配",
            meta: {
                label: "整单 / 未分配",
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
            cell: ({ row }) => <ScopeStatusCell status={row.original.status} />,
        },
    ]
}

/** 范围进项发票分配列：获授权分配常显，整单分配受限时为空。 */
export function createSupplierScopedAllocationColumns(): ColumnDef<ScopedPurchaseInvoiceAllocationWire>[] {
    return [
        {
            id: "doc",
            header: "进项发票 / 应付记录",
            meta: { label: "进项发票 / 应付记录", width: "flex" },
            cell: ({ row }) => {
                const invoice = allocationNumber(row.original)
                const payable = `应付 · ${allocationPayableNumber(row.original)}`
                return (
                    <TableIdentityCell
                        primary={<span className="num">{invoice}</span>}
                        primaryTitle={invoice}
                        secondary={payable}
                        secondaryTitle={payable}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "visible",
            header: "权限内分配",
            meta: {
                label: "权限内分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <ScopeShareCell
                    value={row.original.visible_allocated_amount}
                    limitedId={
                        row.original.permission_limited
                            ? `supplier-payables-scope-allocation-limited-${row.original.id}`
                            : undefined
                    }
                />
            ),
        },
        {
            id: "whole",
            header: "整单分配",
            meta: {
                label: "整单分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.allocated_gross_amount}
                    display="exact"
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
