import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon } from "lucide-react"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CardBusinessAnalyticsView, CardBusinessRow } from "../types"
import { COST_BASIS_ROW_UI } from "../types"

export function useCardBusinessColumns(
    data: CardBusinessAnalyticsView | undefined,
): ColumnDef<CardBusinessRow>[] {
    return React.useMemo<ColumnDef<CardBusinessRow>[]>(
        () => [
            {
                id: "customer",
                accessorFn: (r) => r.customerLabel,
                header: "客户",
                meta: { label: "客户" },
                cell: ({ row }) =>
                    row.original.customerId ? (
                        <Link
                            href={`/sales/customers/${row.original.customerId}`}
                            className="text-sm underline-offset-2 hover:underline"
                        >
                            {row.original.customerLabel}
                        </Link>
                    ) : (
                        <span className="text-sm">
                            {row.original.customerLabel}
                        </span>
                    ),
            },
            {
                id: "salesOrder",
                accessorFn: (r) => r.salesOrderNo ?? "",
                header: "销售单",
                meta: { label: "销售单" },
                cell: ({ row }) =>
                    row.original.salesOrderId ? (
                        <Link
                            href={`/sales/orders/${row.original.salesOrderId}`}
                            className="text-sm underline-offset-2 hover:underline"
                        >
                            {row.original.salesOrderNo}
                        </Link>
                    ) : (
                        <span className="text-sm">
                            {row.original.salesOrderNo ?? "—"}
                        </span>
                    ),
            },
            {
                id: "category",
                accessorFn: (r) => r.voucherCategoryLabel,
                header: "卡券类目",
                meta: { label: "卡券类目" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.voucherCategoryLabel}
                    </span>
                ),
            },
            {
                id: "cardRef",
                accessorFn: (r) => r.cardInstanceRef ?? "",
                header: "卡实例引用",
                meta: { label: "稳定卡实例引用摘要" },
                cell: ({ row }) =>
                    row.original.cardInstanceRef ? (
                        <span
                            className="num text-sm"
                            title="不可逆稳定引用，不可反推卡号/卡密"
                        >
                            {row.original.cardInstanceRef}
                        </span>
                    ) : (
                        <span className="text-sm text-muted-foreground">—</span>
                    ),
            },
            {
                id: "consumption",
                accessorFn: (r) => r.consumptionGross,
                header: "消费(含税)",
                meta: {
                    label: "消费金额含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.consumptionGross} />
                ),
            },
            {
                id: "refund",
                accessorFn: (r) => r.refundGross,
                header: "退款(含税)",
                meta: {
                    label: "退款含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.refundGross} />
                ),
            },
            {
                id: "costBasis",
                accessorFn: (r) => r.costBasis,
                header: "成本口径",
                meta: { label: "成本口径" },
                cell: ({ row }) => {
                    const ui = COST_BASIS_ROW_UI[row.original.costBasis]
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={ui.label}
                            tone={ui.tone}
                        />
                    )
                },
            },
            {
                id: "cost",
                accessorFn: (r) => r.costNet ?? "",
                header: "成本(不含税)",
                meta: {
                    label: "成本不含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => {
                    if (row.original.costBasis === "NONE") {
                        return (
                            <MoneyValue
                                value={null}
                                unavailableReason="无可用成本 · 不显示金额"
                            />
                        )
                    }
                    return <MoneyValue value={row.original.costNet} />
                },
            },
            {
                id: "coverage",
                accessorFn: (r) => r.coverageStatus,
                header: "覆盖",
                meta: { label: "覆盖状态" },
                cell: ({ row }) => {
                    const s = row.original.coverageStatus
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={
                                s === "covered"
                                    ? "已覆盖"
                                    : s === "partial"
                                      ? "部分"
                                      : "未覆盖"
                            }
                            tone={
                                s === "covered"
                                    ? "success"
                                    : s === "partial"
                                      ? "warning"
                                      : "destructive"
                            }
                        />
                    )
                },
            },
            {
                id: "balance",
                accessorFn: (r) => r.unfulfilledBalanceGross,
                header: "未履约余额(含税)",
                meta: {
                    label: "未履约余额含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.unfulfilledBalanceGross} />
                ),
            },
            {
                id: "actions",
                header: "下钻",
                meta: { label: "下钻" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap gap-1">
                        {row.original.consumptionOrderHref ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={row.original.consumptionOrderHref}
                                    />
                                }
                            >
                                {openWorkspaceLabel("W25")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                        {row.original.supplierOrderHref ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={row.original.supplierOrderHref}
                                    />
                                }
                            >
                                {openWorkspaceLabel("W26")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                        {row.original.costBasis === "NONE" && data ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={
                                            data.governanceLinks
                                                .noneCoverageHref
                                        }
                                    />
                                }
                            >
                                {openWorkspaceLabel("W29")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                    </div>
                ),
            },
        ],
        [data],
    )
}
