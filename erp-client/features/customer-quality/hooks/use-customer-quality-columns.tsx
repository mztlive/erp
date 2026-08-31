"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import type {
    BusinessTag,
    CustomerQualityRow,
    CustomerQualityView,
} from "../types"
import {
    customerHref,
    profitLossHref,
    receivablesHref,
    salesOrdersHref,
    withReturnFocus,
} from "../lib/links"

export function useCustomerQualityColumns({
    data,
    returnTo,
    businessType,
    onTagClick,
}: {
    data?: CustomerQualityView
    returnTo: string
    businessType?: "VOUCHER" | "GOODS_SERVICE"
    onTagClick: (tag: BusinessTag) => void
}): ColumnDef<CustomerQualityRow>[] {
    return React.useMemo<ColumnDef<CustomerQualityRow>[]>(
        () => [
            {
                id: "customerName",
                accessorKey: "customerName",
                header: "客户",
                meta: { label: "客户", width: "reference" },
                cell: ({ row }) => {
                    const r = row.original
                    const canW03 = r.allowedDrilldowns.includes("W03")
                    return (
                        <div
                            className="min-w-0"
                            data-customer-row={r.customerId}
                            tabIndex={-1}
                        >
                            <div className="flex flex-wrap items-center gap-2">
                                {canW03 ? (
                                    <Button
                                        id={`customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}-open`}
                                        type="button"
                                        variant="link"
                                        size="xs"
                                        className="h-auto px-0 font-medium"
                                        render={
                                            <Link
                                                href={customerHref(
                                                    r.customerId,
                                                    r.customerName,
                                                    withReturnFocus(
                                                        returnTo,
                                                        r.customerId,
                                                    ),
                                                )}
                                            />
                                        }
                                    >
                                        {r.customerName}
                                    </Button>
                                ) : (
                                    <span className="text-sm font-medium">
                                        {r.customerName}
                                    </span>
                                )}
                                <span className="num text-xs text-muted-foreground">
                                    {r.customerNo}
                                </span>
                            </div>
                            <div className="mt-0.5 text-xs text-muted-foreground">
                                {r.ownerLabels.join(" · ")}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "tags",
                header: "经营标签",
                meta: { label: "经营标签" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap gap-1">
                        {row.original.tags.map((t) => (
                            <button
                                id={`customers-quality-detail-row-${toAutomationIdSegment(row.original.customerId)}-tag-${toAutomationIdSegment(t.type)}-${toAutomationIdSegment(t.code)}`}
                                key={`${t.type}-${t.code}`}
                                type="button"
                                className="inline-flex"
                                onClick={() => onTagClick(t)}
                                aria-label={`${t.label}：查看规则说明`}
                            >
                                <BusinessStatusBadge
                                    context="list"
                                    label={t.label}
                                    tone={t.tone}
                                />
                            </button>
                        ))}
                    </div>
                ),
            },
            {
                id: "salesGrossAmount",
                accessorFn: (r) => r.salesGrossAmount,
                header: "成交金额（含税）",
                meta: {
                    label: "成交金额（含税）",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => {
                    const r = row.original
                    const content = (
                        <div className="text-right">
                            <MoneyValue
                                value={r.salesGrossAmount}
                                taxBasis="gross"
                            />
                            <div className="text-xs text-muted-foreground">
                                {r.salesOrderCount} 单 · 卡券占比{" "}
                                {r.voucherShare}
                            </div>
                        </div>
                    )
                    if (!data || !r.allowedDrilldowns.includes("W05"))
                        return content
                    return (
                        <Link
                            id={`customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}-sales-orders`}
                            data-customer-id={r.customerId}
                            data-focus-metric="salesGrossAmount"
                            href={salesOrdersHref(
                                r,
                                { from: data.period.from, to: data.period.to },
                                withReturnFocus(
                                    returnTo,
                                    r.customerId,
                                    "salesGrossAmount",
                                ),
                                businessType,
                            )}
                            className="block text-right text-primary underline-offset-4 hover:underline"
                        >
                            {content}
                        </Link>
                    )
                },
            },
            {
                id: "costCoverageRate",
                accessorFn: (r) => r.costCoverageRate ?? "",
                header: "成本覆盖",
                meta: { label: "成本覆盖", align: "end" },
                cell: ({ row }) => {
                    const r = row.original
                    if (
                        r.costCoveredNetRevenue == null ||
                        r.costUncoveredNetRevenue == null ||
                        r.costCoverageRate == null
                    ) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                卡券/未覆盖 — 不显示为 0
                            </span>
                        )
                    }
                    return (
                        <div className="text-right text-xs">
                            <div>
                                覆盖{" "}
                                <MoneyValue
                                    value={r.costCoveredNetRevenue}
                                    taxBasis="net"
                                />
                            </div>
                            <div className="text-muted-foreground">
                                未覆盖{" "}
                                <MoneyValue
                                    value={r.costUncoveredNetRevenue}
                                    taxBasis="net"
                                />
                            </div>
                            <div className="num font-medium">
                                {r.costCoverageRate}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "actualProfitLossNet",
                accessorFn: (r) => r.actualProfitLossNet ?? "",
                header: "实际盈亏（不含税）",
                meta: {
                    label: "实际盈亏（不含税）",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => {
                    const r = row.original
                    if (r.actualProfitLossNet == null) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                暂无可靠口径
                            </span>
                        )
                    }
                    const canW16 = r.allowedDrilldowns.includes("W16")
                    const content = (
                        <div className="text-right">
                            <MoneyValue
                                value={r.actualProfitLossNet}
                                taxBasis="net"
                            />
                            {r.marginRate ? (
                                <div className="text-xs text-muted-foreground">
                                    利润率 {r.marginRate}
                                </div>
                            ) : null}
                        </div>
                    )
                    if (!canW16 || !data) return content
                    return (
                        <Link
                            id={`customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}-profit-loss`}
                            data-customer-id={r.customerId}
                            data-focus-metric="actualProfitLossNet"
                            href={profitLossHref(
                                r,
                                { from: data.period.from, to: data.period.to },
                                withReturnFocus(
                                    returnTo,
                                    r.customerId,
                                    "actualProfitLossNet",
                                ),
                            )}
                            className="block text-right text-primary underline-offset-4 hover:underline"
                        >
                            {content}
                        </Link>
                    )
                },
            },
            {
                id: "receivableOpenGross",
                accessorFn: (r) => r.receivableOpenGross ?? "",
                header: "应收 / 逾期（含税）",
                meta: { label: "应收 / 逾期（含税）", align: "end" },
                cell: ({ row }) => {
                    const r = row.original
                    const canW11 = r.allowedDrilldowns.includes("W11")
                    return (
                        <div className="text-right text-xs">
                            <div className="flex flex-wrap items-center justify-end gap-1">
                                <MoneyValue
                                    value={r.receivableOpenGross}
                                    taxBasis="gross"
                                    unavailableReason={
                                        r.receivableOpenGross == null
                                            ? "当前角色不可查看"
                                            : undefined
                                    }
                                />
                                {r.cardFundsReviewInsufficient ? (
                                    <Badge variant="warning">票款未复核</Badge>
                                ) : null}
                            </div>
                            {canW11 && data && r.overdueGross != null ? (
                                <Button
                                    id={`customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}-overdue`}
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="h-auto px-0 text-destructive"
                                    render={
                                        <Link
                                            href={receivablesHref(
                                                r,
                                                {
                                                    from: data.period.from,
                                                    to: data.period.to,
                                                },
                                                withReturnFocus(
                                                    returnTo,
                                                    r.customerId,
                                                    "overdueGross",
                                                ),
                                            )}
                                            data-customer-id={r.customerId}
                                            data-focus-metric="overdueGross"
                                        />
                                    }
                                >
                                    逾期{" "}
                                    <MoneyValue
                                        value={r.overdueGross}
                                        taxBasis="gross"
                                    />
                                </Button>
                            ) : (
                                <div className="text-muted-foreground">
                                    逾期{" "}
                                    <MoneyValue
                                        value={r.overdueGross}
                                        taxBasis="gross"
                                        unavailableReason={
                                            r.overdueGross == null
                                                ? "—"
                                                : undefined
                                        }
                                    />
                                </div>
                            )}
                        </div>
                    )
                },
            },
            {
                id: "exceptions",
                header: "异常",
                meta: { label: "异常" },
                cell: ({ row }) => {
                    const e = row.original.exceptionCounts
                    const parts = [
                        e.return ? `退货 ${e.return}` : null,
                        e.refund ? `退款 ${e.refund}` : null,
                        e.reject ? `拒收 ${e.reject}` : null,
                        e.other ? `其他 ${e.other}` : null,
                    ].filter(Boolean)
                    return (
                        <span className="text-sm text-muted-foreground">
                            {parts.length ? parts.join(" · ") : "—"}
                        </span>
                    )
                },
            },
            {
                id: "latestBusinessAt",
                accessorFn: (r) => r.latestBusinessAt ?? "",
                header: "最近业务",
                meta: { label: "最近业务" },
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {row.original.latestBusinessAt
                            ? formatDateTime(
                                  row.original.latestBusinessAt,
                                  "full",
                                  "passthrough",
                              )
                            : "—"}
                    </span>
                ),
            },
        ],
        [businessType, data, returnTo, onTagClick],
    )
}
