"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
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
                meta: { label: "客户", width: "flex" },
                cell: ({ row }) => {
                    const r = row.original
                    const id = `customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}`
                    const secondary = [r.customerNo, ...r.ownerLabels]
                        .filter(Boolean)
                        .join(" · ")
                    return (
                        <div
                            id={`${id}-identity`}
                            className="min-w-0"
                            data-customer-row={r.customerId}
                            tabIndex={-1}
                        >
                            <TableIdentityCell
                                primary={
                                    r.allowedDrilldowns.includes("W03") ? (
                                        <Link
                                            id={`${id}-open`}
                                            href={customerHref(
                                                r.customerId,
                                                r.customerName,
                                                withReturnFocus(
                                                    returnTo,
                                                    r.customerId,
                                                ),
                                            )}
                                            className="text-primary underline-offset-4 hover:underline focus-visible:underline"
                                        >
                                            {r.customerName}
                                        </Link>
                                    ) : (
                                        r.customerName
                                    )
                                }
                                primaryTitle={r.customerName}
                                secondary={secondary}
                                secondaryTitle={secondary}
                                numericSecondary
                            />
                        </div>
                    )
                },
            },
            {
                id: "tags",
                header: "经营标签",
                meta: { label: "经营标签", width: "default" },
                cell: ({ row }) => {
                    const { tags } = row.original
                    if (tags.length === 0) return <TableEmptyCell />

                    const renderTag = (t: BusinessTag) => (
                        <Button
                            id={`customers-quality-detail-row-${toAutomationIdSegment(row.original.customerId)}-tag-${toAutomationIdSegment(t.type)}-${toAutomationIdSegment(t.code)}`}
                            key={`${t.type}-${t.code}`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-auto max-w-full px-0 py-0.5 font-normal"
                            onClick={(event) => {
                                event.stopPropagation()
                                onTagClick(t)
                            }}
                            aria-label={`${t.label}：查看规则说明`}
                        >
                            <BusinessStatusBadge
                                context="list"
                                label={t.label}
                                tone={t.tone}
                                appearance={
                                    t.tone === "success" ? "quiet" : "badge"
                                }
                            />
                        </Button>
                    )
                    return (
                        <div className="flex flex-col gap-1 whitespace-normal">
                            <div className="flex flex-wrap gap-1">
                                {tags.slice(0, 2).map(renderTag)}
                            </div>
                            {tags.length > 2 ? (
                                <details>
                                    <summary
                                        id={`customers-quality-${toAutomationIdSegment(row.original.customerId)}-more-tags`}
                                        className="cursor-pointer text-xs text-muted-foreground"
                                    >
                                        另{" "}
                                        <span className="num">
                                            {tags.length - 2}
                                        </span>{" "}
                                        项
                                    </summary>
                                    <div className="mt-1 flex flex-wrap gap-1">
                                        {tags.slice(2).map(renderTag)}
                                    </div>
                                </details>
                            ) : null}
                        </div>
                    )
                },
            },
            {
                id: "salesGrossAmount",
                accessorFn: (r) => r.salesGrossAmount,
                header: "成交金额（含税）",
                meta: {
                    label: "成交金额（含税）",
                    align: "end",
                    numeric: true,
                    width: "amount",
                },
                cell: ({ row }) => {
                    const r = row.original
                    const content = (
                        <div className="text-right">
                            <MoneyValue
                                display="compact"
                                value={r.salesGrossAmount}
                                taxBasis="gross"
                            />
                            <div className="text-xs text-muted-foreground">
                                <span className="num">{r.salesOrderCount}</span>{" "}
                                单 · 卡券占比{" "}
                                <span className="num">{r.voucherShare}</span>
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
                meta: {
                    label: "成本覆盖",
                    align: "end",
                    numeric: true,
                    width: "rate",
                },
                cell: ({ row }) => {
                    const r = row.original
                    if (
                        r.costCoveredNetRevenue == null ||
                        r.costUncoveredNetRevenue == null ||
                        r.costCoverageRate == null
                    ) {
                        const isVoucherOnly = /^0+(?:\.0+)?%?$/.test(
                            r.nonVoucherShare.trim(),
                        )
                        return (
                            <TableEmptyCell
                                variant="label"
                                label={
                                    isVoucherOnly
                                        ? "不适用（卡券）"
                                        : "待核算成本"
                                }
                            />
                        )
                    }
                    return (
                        <div className="flex flex-col gap-0.5 text-right">
                            <div className="num font-medium">
                                {r.costCoverageRate}
                            </div>
                            <details>
                                <summary
                                    id={`customers-quality-${toAutomationIdSegment(r.customerId)}-coverage-amounts`}
                                    className="cursor-pointer text-xs text-muted-foreground"
                                >
                                    查看金额
                                </summary>
                                <div>
                                    覆盖{" "}
                                    <MoneyValue
                                        display="compact"
                                        value={r.costCoveredNetRevenue}
                                        taxBasis="net"
                                    />
                                </div>
                                <div className="text-muted-foreground">
                                    未覆盖{" "}
                                    <MoneyValue
                                        display="compact"
                                        value={r.costUncoveredNetRevenue}
                                        taxBasis="net"
                                    />
                                </div>
                            </details>
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
                    width: "amount",
                },
                cell: ({ row }) => {
                    const r = row.original
                    if (r.actualProfitLossNet == null) {
                        return <TableEmptyCell variant="label" label="待核算" />
                    }
                    const canW16 = r.allowedDrilldowns.includes("W16")
                    const content = (
                        <div className="text-right">
                            <MoneyValue
                                display="compact"
                                value={r.actualProfitLossNet}
                                taxBasis="net"
                            />
                            {r.marginRate ? (
                                <div className="text-xs text-muted-foreground">
                                    利润率{" "}
                                    <span className="num">{r.marginRate}</span>
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
                meta: {
                    label: "应收 / 逾期（含税）",
                    align: "end",
                    numeric: true,
                    width: "amount",
                },
                cell: ({ row }) => {
                    const r = row.original
                    const canW11 = r.allowedDrilldowns.includes("W11")
                    return (
                        <div className="flex flex-col gap-0.5 text-right">
                            <div className="flex flex-wrap items-center justify-end gap-1">
                                <MoneyValue
                                    display="compact"
                                    value={r.receivableOpenGross}
                                    taxBasis="gross"
                                    unavailableReason={
                                        r.receivableOpenGross == null
                                            ? "当前角色不可查看"
                                            : undefined
                                    }
                                />
                            </div>
                            {canW11 && data && r.overdueGross != null ? (
                                <Button
                                    id={`customers-quality-detail-row-${toAutomationIdSegment(r.customerId)}-overdue`}
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="h-auto justify-end px-0 text-body-compact text-destructive"
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
                                        display="compact"
                                        value={r.overdueGross}
                                        taxBasis="gross"
                                    />
                                </Button>
                            ) : (
                                <div className="text-muted-foreground">
                                    逾期{" "}
                                    {r.overdueGross == null ? (
                                        <TableEmptyCell />
                                    ) : (
                                        <MoneyValue
                                            display="compact"
                                            value={r.overdueGross}
                                            taxBasis="gross"
                                        />
                                    )}
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
                        <TableTextCell
                            value={parts.join(" · ")}
                            clamp={2}
                            numeric
                            className="text-muted-foreground"
                        />
                    )
                },
            },
            {
                id: "latestBusinessAt",
                accessorFn: (r) => r.latestBusinessAt ?? "",
                header: "最近业务",
                meta: { label: "最近业务", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            row.original.latestBusinessAt
                                ? formatDateTime(
                                      row.original.latestBusinessAt,
                                      "full",
                                      "passthrough",
                                  )
                                : null
                        }
                        numeric
                        className="text-muted-foreground"
                    />
                ),
            },
        ],
        [businessType, data, returnTo, onTagClick],
    )
}
