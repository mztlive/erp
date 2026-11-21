"use client"

import {
    ArrowUpRightIcon,
    ChartNoAxesCombinedIcon,
    CircleAlertIcon,
} from "lucide-react"
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Skeleton } from "@/components/ui/skeleton"
import {
    ChartContainer,
    ChartTooltip,
    ChartTooltipContent,
} from "@/components/ui/chart"
import { compareDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { useWorkspaceOverview } from "../hooks/use-workspace-overview"
import {
    OVERVIEW_LABELS,
    type OverviewData,
    type OverviewMetric,
    type OverviewView,
} from "../lib/overview"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import type { TodayWorkspaceView, WorkspaceMetricKey } from "../types"

export type WorkspaceOverview = ReturnType<typeof useWorkspaceOverview>
export function overviewValue(metric: OverviewMetric, data?: OverviewData) {
    if (!data || data.noScope) return undefined
    return data[metric.field]
}

export function WorkspaceOverviewControls({
    overview,
    onChange,
}: {
    overview: WorkspaceOverview
    onChange: (view: OverviewView, preset: PeriodPreset) => void
}) {
    if (!overview.view) return null
    return (
        <div className="flex flex-wrap items-center gap-2">
            <OptionCombobox
                id="workspace-overview-view"
                aria-label="业务视图"
                value={overview.view}
                options={overview.views.map((view) => ({
                    value: view,
                    label: OVERVIEW_LABELS[view],
                }))}
                onValueChange={(value) => {
                    if (value) onChange(value as OverviewView, overview.preset)
                }}
                getOptionId={(value) => `workspace-overview-option-${value}`}
                allowClear={false}
                retainMissingSelection={false}
                size="sm"
                className="w-40"
            />
            {(overview.view === "sales" || overview.view === "management") && (
                <OptionCombobox
                    id="workspace-overview-period"
                    aria-label="业务统计期间"
                    value={overview.preset}
                    options={[
                        { value: "month-to-date", label: "本月迄今" },
                        { value: "last-month", label: "上月" },
                        { value: "quarter-to-date", label: "本季迄今" },
                    ]}
                    onValueChange={(value) => {
                        if (value)
                            onChange(overview.view!, value as PeriodPreset)
                    }}
                    getOptionId={(value) => `workspace-period-option-${value}`}
                    allowClear={false}
                    size="sm"
                    className="w-36"
                />
            )}
        </div>
    )
}

export function WorkspaceOverviewMetrics({
    overview,
    onSelect,
}: {
    overview: WorkspaceOverview
    onSelect: (metric: OverviewMetric) => void
}) {
    if (!overview.view) return null
    return (
        <section aria-label="岗位业务概览" className="shrink-0 space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <span>当前授权范围 · {OVERVIEW_LABELS[overview.view]}</span>
                <span>
                    {overview.view === "sales" || overview.view === "management"
                        ? `${overview.period.from} 至 ${overview.period.to} · 余额与待办不受期间限制`
                        : "当前存量 · 全部期间"}
                </span>
            </div>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {overview.metrics.map((metric, index) => {
                    const query = overview.queries[index]!
                    const data = query.isError ? undefined : query.data
                    const value = overviewValue(metric, data)
                    const warning =
                        metric.warning &&
                        value != null &&
                        compareDecimal(String(value), "0", 2) > 0
                    return (
                        <button
                            key={metric.key}
                            id={`workspace-overview-${metric.key}`}
                            type="button"
                            onClick={() => onSelect(metric)}
                            className="group min-w-0 rounded-xl border border-border/60 bg-card p-4 text-left transition-colors hover:bg-muted/30 focus-visible:outline-2 focus-visible:outline-ring"
                        >
                            <span className="flex items-center justify-between gap-2 text-body-compact text-muted-foreground">
                                {metric.label}
                                <ArrowUpRightIcon className="size-3.5 shrink-0 opacity-40 group-hover:opacity-100" />
                            </span>
                            <div
                                className={cn(
                                    "num mt-3 min-h-9 truncate text-value-section font-semibold tracking-tight",
                                    warning && "text-destructive",
                                )}
                            >
                                {query.isPending ? (
                                    <Skeleton className="h-8 w-28" />
                                ) : query.isError ? (
                                    <span className="text-body-compact">
                                        读取失败，点击重试
                                    </span>
                                ) : data?.noScope ? (
                                    <span className="text-body-compact">
                                        暂无数据查看权限
                                    </span>
                                ) : value == null ? (
                                    <span className="text-body-compact text-muted-foreground">
                                        {data?.limited
                                            ? "仅统计权限内部分数据"
                                            : "暂不可用"}
                                    </span>
                                ) : metric.field === "count" ? (
                                    <>
                                        {value}
                                        <span className="ml-1 text-xs font-normal text-muted-foreground">
                                            项
                                        </span>
                                    </>
                                ) : (
                                    <MoneyValue
                                        value={String(value)}
                                        display="compact"
                                    />
                                )}
                            </div>
                            <p className="mt-2 text-xs text-muted-foreground">
                                {metric.detail}
                                {data?.scopeSummary
                                    ? ` · ${data.scopeSummary}`
                                    : ""}
                                {data?.asOf
                                    ? ` · 更新于 ${new Date(data.asOf).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })}`
                                    : ""}
                                {data?.limited ? " · 仅统计权限内部分数据" : ""}
                                {query.isFetching && !query.isPending
                                    ? " · 更新中"
                                    : ""}
                            </p>
                        </button>
                    )
                })}
            </div>
        </section>
    )
}

export function WorkspaceInsights({
    overview,
    queue,
    queueFailed = false,
    queueLoading = false,
    onMetricClick,
    onSelect,
}: {
    overview: WorkspaceOverview
    queue?: TodayWorkspaceView
    queueFailed?: boolean
    queueLoading?: boolean
    onMetricClick: (key: WorkspaceMetricKey) => void
    onSelect: (metric: OverviewMetric) => void
}) {
    const overdue = queue?.metrics.find((metric) => metric.key === "overdue")
    const blocked = queue?.metrics.find((metric) => metric.key === "blocked")
    const warnings = overview.metrics.filter((metric, index) => {
        const query = overview.queries[index]!
        const value = query.isError
            ? undefined
            : overviewValue(metric, query.data)
        return (
            metric.warning &&
            value != null &&
            compareDecimal(String(value), "0", 2) > 0
        )
    })
    const businessData = overview.queries.find(
        (query) => query.isSuccess && query.data.trend,
    )?.data
    const trend =
        overview.view === "sales"
            ? overview.trend.data?.points
            : businessData?.trend?.map((point) => ({
                  label: point.label,
                  // fixed-decimal-display-boundary: Recharts coordinates require number; tooltips retain original amounts.
                  value: point.value == null ? null : Number(point.value),
                  amount: point.value,
              }))
    const failed =
        overview.view === "sales"
            ? overview.trend.isError
            : overview.queries.some((query) => query.isError)
    return (
        <aside aria-label="业务分析与风险" className="min-w-0 space-y-4">
            <section className="rounded-xl border border-border/60 bg-card p-4">
                <h2 className="flex items-center gap-2 text-section-title font-semibold tracking-tight">
                    <ChartNoAxesCombinedIcon className="size-4 text-muted-foreground" />
                    {overview.view === "sales"
                        ? "新增销售单趋势"
                        : overview.view === "management"
                          ? "实际经营盈亏趋势"
                          : "业务分布"}
                </h2>
                <p className="mt-1 text-xs text-muted-foreground">
                    {overview.view === "sales"
                        ? "截至所选期末的近六个月 · 按创建日期，含草稿"
                        : overview.view === "management"
                          ? "非卡券 · 不含税 · 所选期间"
                          : "当前授权范围内的业务情况"}
                </p>
                {failed ? (
                    <p
                        role="status"
                        className="py-8 text-xs text-muted-foreground"
                    >
                        分析读取失败，请刷新重试。
                    </p>
                ) : trend?.some((point) => point.value != null) ? (
                    <>
                        <ChartContainer
                            id="workspace-business-trend"
                            config={{
                                value: {
                                    label:
                                        overview.view === "sales"
                                            ? "新增销售单"
                                            : "实际经营盈亏",
                                    color: "var(--chart-1)",
                                },
                            }}
                            className="mt-5 h-44 w-full"
                        >
                            <LineChart
                                data={trend}
                                margin={{ left: 0, right: 12, top: 8 }}
                                accessibilityLayer
                            >
                                <CartesianGrid vertical={false} />
                                <XAxis
                                    dataKey="label"
                                    tickLine={false}
                                    axisLine={false}
                                    tickMargin={8}
                                    minTickGap={12}
                                />
                                <YAxis
                                    width={44}
                                    tickLine={false}
                                    axisLine={false}
                                    allowDecimals={overview.view !== "sales"}
                                />
                                <ChartTooltip
                                    content={
                                        <ChartTooltipContent
                                            formatter={(value, _name, item) => (
                                                <span>
                                                    {"amount" in item.payload &&
                                                    item.payload.amount !=
                                                        null ? (
                                                        <MoneyValue
                                                            value={
                                                                item.payload
                                                                    .amount
                                                            }
                                                        />
                                                    ) : (
                                                        `${value} 项`
                                                    )}
                                                </span>
                                            )}
                                        />
                                    }
                                />
                                <Line
                                    type="monotone"
                                    dataKey="value"
                                    stroke="var(--color-value)"
                                    strokeWidth={2}
                                    dot={{ r: 3 }}
                                    connectNulls={false}
                                    isAnimationActive={false}
                                />
                            </LineChart>
                        </ChartContainer>
                        <details className="mt-3 text-xs">
                            <summary
                                id="workspace-trend-data"
                                className="cursor-pointer text-muted-foreground"
                            >
                                查看趋势数据
                            </summary>
                            <ul className="mt-2 space-y-2">
                                {trend.map((point) => (
                                    <li
                                        key={point.label}
                                        className="flex justify-between gap-3"
                                    >
                                        <span>{point.label}</span>
                                        <span>
                                            {point.value == null ? (
                                                "暂不可用"
                                            ) : "amount" in point &&
                                              point.amount != null ? (
                                                <MoneyValue
                                                    value={String(point.amount)}
                                                />
                                            ) : (
                                                `${point.value} 项`
                                            )}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        </details>
                    </>
                ) : overview.view === "sales" ||
                  overview.view === "management" ? (
                    <p className="py-8 text-xs text-muted-foreground">
                        {overview.queries.some((query) => query.isPending) ||
                        overview.trend.isFetching
                            ? "正在读取趋势…"
                            : businessData?.note || "暂无可展示的趋势数据"}
                    </p>
                ) : (
                    <div className="mt-5 space-y-4">
                        {overview.metrics.map((metric, index) => {
                            const query = overview.queries[index]!
                            const value = query.isError
                                ? undefined
                                : overviewValue(metric, query.data)
                            const max = Math.max(
                                1,
                                ...overview.metrics.map((other, i) =>
                                    other.field === "count"
                                        ? (overview.queries[i]?.data?.count ??
                                          0)
                                        : 0,
                                ),
                            )
                            return (
                                <button
                                    id={`workspace-distribution-${metric.key}`}
                                    key={metric.key}
                                    type="button"
                                    onClick={() => onSelect(metric)}
                                    className="block w-full rounded text-left focus-visible:outline-2 focus-visible:outline-ring"
                                >
                                    <span className="flex justify-between gap-3 text-xs">
                                        <span className="text-muted-foreground">
                                            {metric.label}
                                        </span>
                                        <span className="font-medium tabular-nums">
                                            {value == null ? (
                                                "—"
                                            ) : metric.field === "count" ? (
                                                value
                                            ) : (
                                                <MoneyValue
                                                    value={String(value)}
                                                    display="compact"
                                                />
                                            )}
                                        </span>
                                    </span>
                                    {metric.field === "count" && (
                                        <span className="mt-2 block h-1.5 overflow-hidden rounded-full bg-muted">
                                            <span
                                                className="block h-full rounded-full bg-primary/70"
                                                style={{
                                                    width: `${value == null ? 0 : Math.min(100, (Number(value) / max) * 100)}%`,
                                                }}
                                            />
                                        </span>
                                    )}
                                </button>
                            )
                        })}
                    </div>
                )}
                {businessData?.note && (
                    <p className="mt-3 text-xs text-muted-foreground">
                        {businessData.note}
                    </p>
                )}
            </section>
            <section className="rounded-xl border border-border/60 bg-card p-4">
                <h2 className="flex items-center gap-2 text-section-title font-semibold tracking-tight">
                    <CircleAlertIcon className="size-4 text-muted-foreground" />
                    风险与关注
                </h2>
                <div className="mt-3 space-y-2">
                    {[overdue, blocked]
                        .filter((metric) => metric?.visible && metric.count > 0)
                        .map(
                            (metric) =>
                                metric && (
                                    <Button
                                        key={metric.key}
                                        id={`workspace-risk-${metric.key}`}
                                        variant="ghost"
                                        className="h-auto w-full justify-between whitespace-normal px-2 py-3 text-left"
                                        onClick={() =>
                                            onMetricClick(metric.key)
                                        }
                                    >
                                        <span>
                                            {metric.key === "overdue"
                                                ? "我的超期待办"
                                                : "我的受阻事项"}
                                        </span>
                                        <span className="tabular-nums text-destructive">
                                            {metric.count} 项 →
                                        </span>
                                    </Button>
                                ),
                        )}
                    {warnings.map((metric) => (
                        <Button
                            key={metric.key}
                            id={`workspace-risk-${toAutomationIdSegment(metric.key)}`}
                            variant="ghost"
                            className="h-auto w-full justify-between whitespace-normal px-2 py-3 text-left"
                            onClick={() => onSelect(metric)}
                        >
                            <span>{metric.label}</span>
                            <span className="text-destructive">查看明细 →</span>
                        </Button>
                    ))}
                    {!(overdue?.count || blocked?.count || warnings.length) && (
                        <p className="py-3 text-xs text-muted-foreground">
                            {queueFailed ||
                            overview.queries.some((query) => query.isError)
                                ? "部分业务数据读取失败，风险信息可能不完整。"
                                : queueLoading ||
                                    overview.queries.some(
                                        (query) => query.isPending,
                                    )
                                  ? "正在读取业务与待办提醒…"
                                  : "当前已接入的指标未发现需要提醒的事项。"}
                        </p>
                    )}
                </div>
            </section>
        </aside>
    )
}
