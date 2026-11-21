"use client"

import {
    AlertCircleIcon,
    ArrowUpRightIcon,
    ClipboardCheckIcon,
    ClipboardListIcon,
    ClockIcon,
    FileTextIcon,
    ReceiptIcon,
    ShoppingCartIcon,
    TrendingUpIcon,
    TruckIcon,
    WalletCardsIcon,
    type LucideIcon,
} from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { OverviewMetric } from "../lib/overview"
import { overviewValue, type WorkspaceOverview } from "./workspace-overview"
import { WorkbenchDataInfo, workbenchTime } from "./workbench-data-info"

function metricVisual(
    metric: OverviewMetric,
    warning: boolean,
): { icon: LucideIcon; badgeClass: string } {
    if (warning) {
        return {
            icon: AlertCircleIcon,
            badgeClass: "bg-destructive-soft text-destructive-soft-foreground",
        }
    }
    if (metric.key.includes("review")) {
        return {
            icon: ClockIcon,
            badgeClass: "bg-warning-soft text-warning-soft-foreground",
        }
    }
    if (metric.source.kind === "sales") {
        return {
            icon: ShoppingCartIcon,
            badgeClass: "bg-info-soft text-info-soft-foreground",
        }
    }
    if (metric.source.kind === "inquiry") {
        return {
            icon: ClipboardListIcon,
            badgeClass: "bg-indigo-soft text-indigo-soft-foreground",
        }
    }
    if (metric.source.kind === "purchase") {
        return {
            icon: ClipboardCheckIcon,
            badgeClass: "bg-info-soft text-info-soft-foreground",
        }
    }
    if (
        metric.source.kind === "receivable" ||
        metric.source.kind === "receipt"
    ) {
        return {
            icon: ReceiptIcon,
            badgeClass: "bg-success-soft text-success-soft-foreground",
        }
    }
    if (metric.source.kind === "payable" || metric.source.kind === "payment") {
        return {
            icon: WalletCardsIcon,
            badgeClass: "bg-warning-soft text-warning-soft-foreground",
        }
    }
    if (metric.source.kind === "fulfillment") {
        return {
            icon: TruckIcon,
            badgeClass: "bg-teal-soft text-teal-soft-foreground",
        }
    }
    if (metric.source.kind === "profit") {
        return {
            icon: TrendingUpIcon,
            badgeClass: "bg-success-soft text-success-soft-foreground",
        }
    }
    return {
        icon: FileTextIcon,
        badgeClass: "bg-info-soft text-info-soft-foreground",
    }
}

export function RoleWorkbenchMetrics({
    overview,
    onSelect,
    idPrefix,
}: {
    overview: WorkspaceOverview
    onSelect: (metric: OverviewMetric) => void
    idPrefix: string
}) {
    return (
        <div
            className={cn(
                "grid gap-3 sm:grid-cols-2",
                overview.metrics.length > 4
                    ? "xl:grid-cols-3 2xl:grid-cols-5"
                    : "xl:grid-cols-4",
            )}
        >
            {overview.metrics.map((metric, index) => {
                const query = overview.queries[index]!
                const data = query.isError ? undefined : query.data
                const value = overviewValue(metric, data)
                const period = metric.key === "sales-created"
                const warning = Boolean(
                    metric.warning &&
                    value != null &&
                    compareDecimal(String(value), "0", 2) > 0,
                )
                const visual = metricVisual(metric, warning)
                const Icon = visual.icon
                return (
                    <article
                        key={metric.key}
                        className={cn(
                            "group/kpi relative min-w-0 rounded-lg border border-border/90 bg-card shadow-2xs transition-[border-color,box-shadow] duration-150 hover:border-primary/45 hover:shadow-xs",
                            warning &&
                                "border-destructive-border hover:border-destructive/60",
                        )}
                    >
                        <button
                            id={`workspace-overview-${metric.key}`}
                            type="button"
                            onClick={() =>
                                query.isError
                                    ? void query.refetch()
                                    : onSelect(metric)
                            }
                            className="flex h-full w-full flex-col rounded-lg p-4 text-left focus-visible:outline-2 focus-visible:outline-primary"
                        >
                            <div className="flex w-full items-center gap-2 pr-7">
                                <span
                                    className={cn(
                                        "flex size-6.5 shrink-0 items-center justify-center rounded-md",
                                        visual.badgeClass,
                                    )}
                                    aria-hidden="true"
                                >
                                    <Icon className="size-3.5" />
                                </span>
                                <span
                                    className={cn(
                                        "truncate text-body-compact font-medium text-muted-foreground",
                                        warning &&
                                            "text-destructive-soft-foreground",
                                    )}
                                >
                                    {metric.label}
                                </span>
                            </div>
                            <div
                                className={cn(
                                    "mt-3 flex min-h-9 items-baseline gap-1.5 text-value-summary font-semibold num tracking-tight text-foreground",
                                    warning &&
                                        "text-destructive-soft-foreground",
                                )}
                            >
                                {query.isPending ? (
                                    <Skeleton className="h-8 w-24" />
                                ) : query.isError ? (
                                    <span className="text-body-compact font-normal text-destructive">
                                        读取失败，点击重试
                                    </span>
                                ) : data?.noScope ? (
                                    <span className="text-body-compact font-normal text-muted-foreground">
                                        暂无数据查看权限
                                    </span>
                                ) : value == null ? (
                                    <span className="text-body-compact font-normal text-muted-foreground">
                                        {data?.limited
                                            ? "仅统计权限内部分数据"
                                            : "暂不可用"}
                                    </span>
                                ) : metric.field === "count" ? (
                                    <>
                                        {value}
                                        <span className="text-xs font-normal text-muted-foreground">
                                            {metric.source.kind === "inquiry"
                                                ? "项"
                                                : "单"}
                                        </span>
                                    </>
                                ) : (
                                    <MoneyValue
                                        value={String(value)}
                                        display="compact"
                                        size="summary"
                                        showHint={false}
                                    />
                                )}
                            </div>
                            <div className="mt-auto flex w-full items-center justify-between gap-2 pt-3 text-xs text-muted-foreground">
                                <span className="truncate">
                                    {period
                                        ? "所选期间 · 含草稿"
                                        : metric.field === "overdue"
                                          ? "当前逾期余额 · 含税"
                                          : metric.field === "balance"
                                            ? "当前余额 · 含税"
                                            : "截至当前"}
                                    {data?.limited ? " · 部分范围" : ""}
                                </span>
                                <span className="inline-flex shrink-0 items-center gap-0.5 text-muted-foreground/70 transition-colors group-hover/kpi:text-primary">
                                    <ArrowUpRightIcon
                                        className="size-3.5 transition-transform duration-150 group-hover/kpi:translate-x-0.5 group-hover/kpi:-translate-y-0.5"
                                        aria-hidden="true"
                                    />
                                </span>
                            </div>
                        </button>
                        <div className="absolute top-2.5 right-2.5">
                            <WorkbenchDataInfo
                                id={`${idPrefix}-metric-info-${metric.key}`}
                                title={metric.label}
                            >
                                <p>{metric.detail}</p>
                                <p>
                                    范围：
                                    {data?.scopeSummary ||
                                        "以该业务模块的授权范围为准"}
                                </p>
                                <p>
                                    {period
                                        ? `${overview.period.from} 至 ${overview.period.to}`
                                        : overview.view === "sales"
                                          ? "截至当前，不随新增单据期间变化。"
                                          : "截至当前，包含全部期间的当前存量。"}
                                </p>
                                {metric.field !== "count" && value != null && (
                                    <p>
                                        完整金额：
                                        <MoneyValue
                                            value={String(value)}
                                            display="exact"
                                        />
                                    </p>
                                )}
                                <p>
                                    更新时间：{workbenchTime(data?.asOf, true)}
                                </p>
                                {data?.limited && (
                                    <p>当前仅统计权限内部分数据。</p>
                                )}
                                {data?.note && <p>{data.note}</p>}
                            </WorkbenchDataInfo>
                        </div>
                    </article>
                )
            })}
        </div>
    )
}
