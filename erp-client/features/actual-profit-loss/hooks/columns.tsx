import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { formatDateTime } from "@/lib/datetime"
import { BusinessStatusBadge } from "@/components/business"
import { MoneyCell } from "@/features/actual-profit-loss/components/money-cell"
import {
    COVERAGE_STATE_UI,
    type ProfitLossRow,
} from "@/features/actual-profit-loss/types"

/**
 * 盈亏明细列定义。openCostDetail / rowFocusRef 由页面持有（下钻与焦点恢复状态），
 * 列定义本身无状态；表头排序 ↔ URL sort 的接线保留在页面。
 */
export function buildProfitLossColumns(options: {
    openCostDetail: (row: ProfitLossRow) => void
    rowFocusRef: React.MutableRefObject<Map<string, HTMLElement | null>>
}): ColumnDef<ProfitLossRow>[] {
    const { openCostDetail, rowFocusRef } = options
    return [
        {
            id: "identityLabel",
            accessorFn: (r) => r.identityLabel,
            header: "销售单号",
            meta: { label: "销售单号", width: "default" as const },
            cell: ({ row }) => {
                const r = row.original
                const href = r.objectId
                    ? `/sales/orders/${encodeURIComponent(r.objectId)}`
                    : undefined
                return (
                    <div className="flex flex-col gap-0.5">
                        {href ? (
                            <Link
                                href={href}
                                className="font-medium text-primary underline-offset-2 hover:underline"
                                ref={(el) => {
                                    rowFocusRef.current.set(r.rowId, el)
                                }}
                            >
                                {r.identityLabel}
                            </Link>
                        ) : (
                            <span className="font-medium">
                                {r.identityLabel}
                            </span>
                        )}
                        <span className="text-xs text-muted-foreground">
                            {r.customerLabel}
                        </span>
                    </div>
                )
            },
        },
        {
            id: "benefitScenarios",
            accessorFn: (r) => r.benefitScenarios?.join("、") ?? "",
            header: "福利场景",
            meta: { label: "福利场景" },
            cell: ({ row }) => (
                <span className="text-sm text-muted-foreground">
                    {row.original.benefitScenarios?.join("、") || "—"}
                </span>
            ),
        },
        {
            id: "fulfillmentModes",
            accessorFn: (r) => r.fulfillmentModes?.join("、") ?? "",
            header: "履约方式",
            meta: { label: "履约方式" },
            cell: ({ row }) => (
                <span className="text-sm">
                    {row.original.fulfillmentModes?.join("、") || "—"}
                </span>
            ),
        },
        {
            id: "netSalesRevenue",
            accessorFn: (r) => r.netSalesRevenue,
            header: "不含税收入",
            meta: {
                label: "不含税收入",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => (
                <MoneyCell value={row.original.netSalesRevenue} />
            ),
        },
        {
            id: "actualProcurementCostNet",
            accessorFn: (r) => r.actualProcurementCostNet ?? "",
            header: "实际采购成本",
            meta: {
                label: "实际采购成本（不含税）",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => {
                const r = row.original
                if (r.coverageState === "UNCOVERED") {
                    return (
                        <span className="text-sm text-muted-foreground">
                            未覆盖
                        </span>
                    )
                }
                if (r.actualProcurementCostNet == null) {
                    return (
                        <span className="text-sm text-muted-foreground">
                            无权限
                        </span>
                    )
                }
                return (
                    <MoneyCell
                        value={r.actualProcurementCostNet}
                        negativeAsText={false}
                        onClick={
                            r.allowedDrilldowns.includes("cost_entry") &&
                            r.costEntryIds.length > 0
                                ? () => openCostDetail(r)
                                : undefined
                        }
                    />
                )
            },
        },
        {
            id: "actualFulfillmentCostNet",
            accessorFn: (r) => r.actualFulfillmentCostNet ?? "",
            header: "实际履约费用",
            meta: {
                label: "实际履约费用（不含税）",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => {
                const r = row.original
                if (r.coverageState === "UNCOVERED") {
                    return (
                        <span className="text-sm text-muted-foreground">
                            未覆盖
                        </span>
                    )
                }
                if (r.actualFulfillmentCostNet == null) {
                    return (
                        <span className="text-sm text-muted-foreground">
                            无权限
                        </span>
                    )
                }
                return (
                    <MoneyCell
                        value={r.actualFulfillmentCostNet}
                        negativeAsText={false}
                        onClick={
                            r.allowedDrilldowns.includes("cost_entry") &&
                            r.costEntryIds.length > 0
                                ? () => openCostDetail(r)
                                : undefined
                        }
                    />
                )
            },
        },
        {
            id: "reductionsNet",
            accessorFn: (r) => r.reductionsNet ?? "",
            header: "成本冲减（负值＝冲减）",
            meta: {
                label: "成本冲减",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => {
                const r = row.original
                if (r.reductionsNet == null) {
                    return (
                        <span className="text-sm text-muted-foreground">—</span>
                    )
                }
                return (
                    <MoneyCell value={r.reductionsNet} negativeAsText={false} />
                )
            },
        },
        {
            id: "actualProfitLossNet",
            accessorFn: (r) => r.actualProfitLossNet ?? "",
            header: "实际盈亏",
            meta: {
                label: "实际盈亏（不含税）",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => {
                const r = row.original
                if (r.actualProfitLossNet == null) {
                    return (
                        <span
                            className="text-sm text-muted-foreground"
                            title={r.marginUnavailableReason}
                        >
                            {r.coverageState === "UNCOVERED"
                                ? "不可用（未覆盖）"
                                : (r.marginUnavailableReason ?? "不可用")}
                        </span>
                    )
                }
                const href = r.objectId
                    ? `/sales/orders/${encodeURIComponent(r.objectId)}`
                    : undefined
                return <MoneyCell value={r.actualProfitLossNet} href={href} />
            },
        },
        {
            id: "marginRate",
            accessorFn: (r) => r.marginRate ?? "",
            header: "实际利润率",
            meta: {
                label: "实际利润率",
                numeric: true,
                align: "end" as const,
            },
            cell: ({ row }) => {
                const r = row.original
                if (r.marginRate == null) {
                    return (
                        <span
                            className="text-sm text-muted-foreground"
                            title={r.marginUnavailableReason}
                        >
                            {r.marginUnavailableReason ?? "不适用"}
                        </span>
                    )
                }
                return <span className="num text-sm">{r.marginRate}</span>
            },
        },
        {
            id: "coverageState",
            accessorFn: (r) => r.coverageState,
            header: "覆盖状态",
            meta: { label: "覆盖状态" },
            cell: ({ row }) => {
                const r = row.original
                const ui = COVERAGE_STATE_UI[r.coverageState]
                const reason = r.coverageBlockers
                    .map((b) => b.message)
                    .join("；")
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={ui.label}
                        tone={ui.tone}
                        description={reason || undefined}
                    />
                )
            },
        },
        {
            id: "latestCostOccurredAt",
            accessorFn: (r) => r.latestCostOccurredAt ?? "",
            header: "最近成本发生",
            meta: { label: "最近成本发生" },
            cell: ({ row }) => (
                <span className="num text-xs text-muted-foreground">
                    {formatDateTime(row.original.latestCostOccurredAt, "full")}
                </span>
            ),
        },
    ]
}
