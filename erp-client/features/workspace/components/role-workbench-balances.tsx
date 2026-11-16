"use client"

import { ArrowUpRightIcon } from "lucide-react"
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
            className="min-w-0 rounded-xl border border-border/70 bg-card"
            aria-labelledby="cashier-workbench-balances-title"
        >
            <div className="flex items-center justify-between px-4 pt-3">
                <div>
                    <h2
                        id="cashier-workbench-balances-title"
                        className="text-sm font-semibold"
                    >
                        待分配资金
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        当前未分配余额 · 含税
                    </p>
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
            <div className="space-y-2 p-3">
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
                        <div key={item.key}>
                            <button
                                id={`role-stage-${item.key.toLowerCase()}`}
                                type="button"
                                onClick={() =>
                                    query.isError
                                        ? void query.refetch()
                                        : onSelect(item)
                                }
                                className="group w-full rounded-lg px-2 py-3 text-left hover:bg-muted/40 focus-visible:outline-2 focus-visible:outline-ring"
                            >
                                <span className="flex items-center justify-between text-xs text-muted-foreground">
                                    {item.label}
                                    <ArrowUpRightIcon
                                        className="size-3.5 opacity-40 group-hover:opacity-100"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span className="mt-2 block min-h-8">
                                    {query.isPending ? (
                                        <Skeleton className="h-7 w-28" />
                                    ) : query.isError ? (
                                        <span className="text-sm text-destructive">
                                            读取失败，点击重试
                                        </span>
                                    ) : query.data?.noScope ? (
                                        <span className="text-sm text-muted-foreground">
                                            暂无数据查看权限
                                        </span>
                                    ) : value == null ? (
                                        <span className="text-sm text-muted-foreground">
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
                                        className="mt-2 block h-2 overflow-hidden rounded-full bg-muted"
                                        aria-hidden="true"
                                    >
                                        <span
                                            className={`block h-full origin-left rounded-full ${item.source.kind === "receipt" ? "bg-info" : "bg-foreground/30"}`}
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
                        </div>
                    )
                })}
                <p className="border-t border-grid px-1 pt-2 text-xs text-muted-foreground">
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
