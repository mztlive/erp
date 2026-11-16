"use client"

import { ArrowUpRightIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { OverviewMetric } from "../lib/overview"
import { overviewValue, type WorkspaceOverview } from "./workspace-overview"
import { WorkbenchDataInfo, workbenchTime } from "./workbench-data-info"

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
                const warning =
                    metric.warning &&
                    value != null &&
                    compareDecimal(String(value), "0", 2) > 0
                return (
                    <article
                        key={metric.key}
                        className={cn(
                            "relative min-w-0 rounded-xl border border-border/70 bg-card",
                            warning && "border-destructive/30",
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
                            className="group flex h-full w-full flex-col rounded-xl p-4 text-left transition-colors hover:bg-muted/30 focus-visible:outline-2 focus-visible:outline-ring"
                        >
                            <span
                                className={cn(
                                    "pr-7 text-sm text-muted-foreground",
                                    warning && "text-destructive",
                                )}
                            >
                                {metric.label}
                            </span>
                            <div className="mt-3 flex min-h-10 items-baseline gap-1.5 text-value-summary font-semibold tabular-nums tracking-tight">
                                {query.isPending ? (
                                    <Skeleton className="h-9 w-24" />
                                ) : query.isError ? (
                                    <span className="text-sm text-destructive">
                                        读取失败，点击重试
                                    </span>
                                ) : data?.noScope ? (
                                    <span className="text-sm text-muted-foreground">
                                        暂无数据查看权限
                                    </span>
                                ) : value == null ? (
                                    <span className="text-sm text-muted-foreground">
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
                            <span className="mt-2 flex w-full items-center justify-between gap-2 text-xs text-muted-foreground">
                                <span>
                                    {period
                                        ? "所选期间 · 含草稿"
                                        : metric.field === "overdue"
                                          ? "当前逾期余额 · 含税"
                                          : metric.field === "balance"
                                            ? "当前余额 · 含税"
                                            : "截至当前"}
                                </span>
                                <ArrowUpRightIcon
                                    className="size-3.5 opacity-40 transition-opacity group-hover:opacity-100"
                                    aria-hidden="true"
                                />
                            </span>
                            <span className="mt-2 text-xs text-muted-foreground">
                                {query.isFetching && !query.isPending
                                    ? "更新中…"
                                    : data?.asOf
                                      ? `更新于 ${workbenchTime(data.asOf)}`
                                      : query.isPending
                                        ? "读取中…"
                                        : "更新时间待返回"}
                                {data?.limited ? " · 仅统计权限内部分数据" : ""}
                            </span>
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
