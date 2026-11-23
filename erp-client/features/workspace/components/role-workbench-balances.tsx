"use client"

import { ArrowUpRightIcon, WalletCardsIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { compareDecimal, divideFixed } from "@/lib/fixed-decimal"
import type { useRoleWorkbench } from "../hooks/use-role-workbench"
import type { OverviewMetric } from "../lib/overview"
import { overviewValue } from "./workspace-overview"
import {
    WorkbenchDataInfo,
    workbenchTime,
    workbenchUpdatedAt,
} from "./workbench-data-info"

export function RoleWorkbenchBalances({
    role,
    onSelect,
}: {
    role: ReturnType<typeof useRoleWorkbench>
    onSelect: (metric: OverviewMetric) => void
}) {
    const maximum = role.stages.reduce((max, query) => {
        const value =
            !query.isError && !query.data?.noScope ? query.data?.balance : null
        return value != null && compareDecimal(value, max, 2) > 0 ? value : max
    }, "0")
    return (
        <section
            className="min-w-0 py-5"
            aria-labelledby="cashier-workbench-balances-title"
        >
            <div className="flex items-center justify-between gap-2 pb-3.5">
                <div className="flex items-center gap-2">
                    <span
                        className="flex size-6 shrink-0 items-center justify-center rounded-md bg-info-soft text-info-soft-foreground"
                        aria-hidden="true"
                    >
                        <WalletCardsIcon className="size-3.5" />
                    </span>
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                        <h2
                            id="cashier-workbench-balances-title"
                            className="text-section-title font-semibold tracking-tight text-foreground"
                        >
                            待分配资金
                        </h2>
                        <span className="text-xs text-muted-foreground">
                            当前未分配余额 · 含税
                        </span>
                    </div>
                </div>
                <WorkbenchDataInfo
                    id="cashier-workbench-balances-info"
                    title="待分配资金"
                >
                    <p>
                        回款待分配与付款待分配分别按各自授权范围展示，不相加、不相减。资金分配不增加收付款发生额。
                    </p>
                    <p>展示当前尚未核销的客户回款与供应商付款余额。</p>
                    {role.metrics.map((item, index) => {
                        const query = role.stages[index]!
                        const value = query.isError
                            ? undefined
                            : overviewValue(item, query.data)
                        return (
                            <div key={item.key}>
                                <p className="font-medium text-foreground">
                                    {item.label}
                                </p>
                                <p>
                                    {query.data?.scopeSummary ||
                                        "以该资金业务授权范围为准"}
                                </p>
                                {value != null && (
                                    <p>
                                        完整金额：
                                        <MoneyValue
                                            value={String(value)}
                                            display="exact"
                                        />
                                    </p>
                                )}
                                <p>
                                    {query.isError
                                        ? "读取失败"
                                        : `更新于 ${workbenchTime(query.data?.asOf, true)}`}
                                </p>
                            </div>
                        )
                    })}
                </WorkbenchDataInfo>
            </div>
            <div className="space-y-3">
                <div className="grid gap-3 sm:grid-cols-2">
                    {role.metrics.map((item, index) => {
                        const query = role.stages[index]!
                        const value = query.isError
                            ? undefined
                            : overviewValue(item, query.data)
                        const ratio =
                            value != null &&
                            compareDecimal(maximum, "0", 2) > 0 &&
                            compareDecimal(String(value), "0", 2) > 0
                                ? divideFixed(String(value), maximum, {
                                      numeratorMaxScale: 2,
                                      denominatorMaxScale: 2,
                                      outputScale: 4,
                                  })
                                : "0"
                        return (
                            <button
                                key={item.key}
                                id={`role-stage-${item.key.toLowerCase()}`}
                                type="button"
                                onClick={() =>
                                    query.isError
                                        ? void query.refetch()
                                        : onSelect(item)
                                }
                                className="group w-full rounded-md bg-muted/35 px-3.5 py-3 text-left transition-colors hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-primary"
                            >
                                <span className="flex items-center justify-between text-xs font-medium text-muted-foreground group-hover:text-primary">
                                    {item.label}
                                    <ArrowUpRightIcon
                                        className="size-3.5 opacity-50 transition-all group-hover:text-primary group-hover:opacity-100"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span className="mt-2 block min-h-8">
                                    {query.isPending ? (
                                        <Skeleton className="h-7 w-28" />
                                    ) : query.isError ? (
                                        <span className="text-body-compact text-destructive">
                                            读取失败，点击重试
                                        </span>
                                    ) : query.data?.noScope ? (
                                        <span className="text-body-compact text-muted-foreground">
                                            暂无数据查看权限
                                        </span>
                                    ) : value == null ? (
                                        <span className="text-body-compact text-muted-foreground">
                                            {query.data?.limited
                                                ? "仅统计权限内部分数据"
                                                : "暂不可用"}
                                        </span>
                                    ) : (
                                        <MoneyValue
                                            value={String(value)}
                                            display="compact"
                                            size="section"
                                            showHint={false}
                                        />
                                    )}
                                </span>
                                {value != null && (
                                    <span
                                        className="mt-2 block h-1.5 overflow-hidden rounded-full bg-muted"
                                        aria-hidden="true"
                                    >
                                        <span
                                            className={`block h-full origin-left rounded-full ${item.source.kind === "receipt" ? "bg-primary" : "bg-success"}`}
                                            style={{
                                                transform: `scaleX(${ratio})`,
                                            }}
                                        />
                                    </span>
                                )}
                                {query.data?.limited && (
                                    <span className="mt-2 block text-xs text-muted-foreground">
                                        仅统计权限内部分数据
                                    </span>
                                )}
                            </button>
                        )
                    })}
                </div>
                <p className="text-xs text-muted-foreground">
                    {role.stages.some((query) => query.isFetching)
                        ? "更新中…"
                        : workbenchUpdatedAt(
                              role.stages.map((query) =>
                                  query.isError ? undefined : query.data?.asOf,
                              ),
                          )}
                </p>
            </div>
        </section>
    )
}
