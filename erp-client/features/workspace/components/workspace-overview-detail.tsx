"use client"

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useRouter } from "@/lib/navigation"
import { MoneyValue, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
    SheetDescription,
} from "@/components/ui/sheet"
import { fetchOverviewData } from "../api/overview"
import { overviewIdentity } from "../hooks/use-workspace-overview"
import type { OverviewMetric } from "../lib/overview"
import type { AccountProfile } from "@/features/auth/api"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { overviewValue } from "./workspace-overview"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function WorkspaceOverviewDetail({
    open,
    metric,
    profile,
    onClose,
    onClosed,
    onTask,
}: {
    open: boolean
    metric: OverviewMetric | null
    profile?: AccountProfile
    onClose: () => void
    onClosed: () => void
    onTask: (id: string) => void
}) {
    return (
        <Sheet
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            onOpenChangeComplete={(nextOpen) => {
                if (!nextOpen) onClosed()
            }}
        >
            <SheetContent
                size="detail"
                closeButtonId="workspace-business-detail-close"
                finalFocus={false}
                className="min-h-0"
            >
                <SheetHeader>
                    <SheetTitle>{metric?.label || "业务明细"}</SheetTitle>
                    <SheetDescription>{metric?.detail}</SheetDescription>
                </SheetHeader>
                {metric && profile && (
                    <BusinessDetailContent
                        key={`${metric.key}:${JSON.stringify(metric.source)}`}
                        metric={metric}
                        profile={profile}
                        onTask={onTask}
                    />
                )}
            </SheetContent>
        </Sheet>
    )
}

function BusinessDetailContent({
    metric,
    profile,
    onTask,
}: {
    metric: OverviewMetric
    profile: AccountProfile
    onTask: (id: string) => void
}) {
    const router = useRouter()
    const [page, setPage] = useState(1)
    const [scopeVersion, setScopeVersion] = useState<string>()
    const query = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "overview-detail",
            overviewIdentity(profile),
            metric.source,
            page,
            scopeVersion,
        ],
        queryFn: () => fetchOverviewData(metric.source, page, scopeVersion),
        retry: false,
    })
    const data = query.isError ? undefined : query.data
    const nextPage = (value: number) => {
        setScopeVersion(value === 1 ? undefined : data?.scopeVersion)
        setPage(value)
    }
    return (
        <>
            <div className="min-h-0 flex-1 overflow-auto px-7 pb-5">
                {query.isPending ? (
                    <p
                        role="status"
                        className="py-8 text-sm text-muted-foreground"
                    >
                        正在读取业务明细…
                    </p>
                ) : query.isError ? (
                    <BusinessFailureState
                        title="业务明细读取失败"
                        error={query.error}
                        onRetry={() => {
                            setPage(1)
                            setScopeVersion(undefined)
                            if (page === 1 && scopeVersion == null)
                                void query.refetch()
                        }}
                    />
                ) : (
                    <>
                        <p className="mb-2 text-sm font-medium">
                            {metric.label}：
                            {overviewValue(metric, data) == null ? (
                                "暂不可用"
                            ) : metric.field === "count" ? (
                                `${data?.count} 项`
                            ) : (
                                <MoneyValue
                                    value={String(overviewValue(metric, data))}
                                />
                            )}
                        </p>
                        <p className="mb-4 text-xs leading-5 text-muted-foreground">
                            {data?.scopeSummary || "当前授权范围"}
                            {data?.asOf
                                ? ` · 数据时间 ${new Date(data.asOf).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })}`
                                : ""}
                        </p>
                        {data?.note && (
                            <p className="mb-4 rounded-lg bg-muted/40 p-3 text-sm leading-6">
                                {data.note}
                            </p>
                        )}
                        {metric.field === "overdue" && (
                            <p className="mb-4 text-xs leading-5 text-muted-foreground">
                                逾期金额按应收分录的到期日计算。下方展示当前授权范围全部应收子账（含未逾期）的当前余额，进入客户往来查看分录与逾期构成。
                            </p>
                        )}
                        {!data?.rows.length ? (
                            <p className="py-10 text-center text-sm text-muted-foreground">
                                {data?.noScope
                                    ? "当前角色无数据范围"
                                    : "当前范围暂无业务记录"}
                            </p>
                        ) : (
                            <ul className="divide-y divide-border/60">
                                {data.rows.map((row) => {
                                    const amount = row.amounts
                                        ? row.amounts[metric.field]
                                        : row.amount
                                    return (
                                        <li
                                            key={row.id}
                                            className="flex flex-wrap items-center justify-between gap-3 py-4"
                                        >
                                            <div className="min-w-0 flex-1">
                                                <p className="break-words text-sm font-medium">
                                                    {row.title}
                                                </p>
                                                <p className="mt-1 break-words text-xs text-muted-foreground">
                                                    {row.description}
                                                    {row.status
                                                        ? ` · ${row.status}`
                                                        : ""}
                                                </p>
                                            </div>
                                            {amount !== undefined && (
                                                <span className="text-sm tabular-nums">
                                                    {amount == null ? (
                                                        "金额受限或暂不可用"
                                                    ) : (
                                                        <MoneyValue
                                                            value={amount}
                                                        />
                                                    )}
                                                </span>
                                            )}
                                            {(row.taskId || row.href) && (
                                                <Button
                                                    id={`workspace-business-row-${toAutomationIdSegment(row.id)}`}
                                                    size="sm"
                                                    variant="outline"
                                                    onClick={() =>
                                                        row.taskId
                                                            ? onTask(row.taskId)
                                                            : row.href &&
                                                              router.push(
                                                                  row.href,
                                                              )
                                                    }
                                                >
                                                    {" "}
                                                    {row.taskId
                                                        ? "处理"
                                                        : "查看"}{" "}
                                                </Button>
                                            )}
                                        </li>
                                    )
                                })}
                            </ul>
                        )}
                    </>
                )}
            </div>
            <footer className="flex shrink-0 items-center justify-between gap-3 border-t px-7 py-4 text-xs text-muted-foreground">
                <span>{data ? `共 ${data.count} 项` : "业务明细"}</span>
                <div className="flex items-center gap-2">
                    <Button
                        id="workspace-business-previous"
                        variant="outline"
                        size="sm"
                        disabled={page <= 1 || query.isFetching}
                        onClick={() => nextPage(page - 1)}
                    >
                        上一页
                    </Button>
                    <span>第 {page} 页</span>
                    <Button
                        id="workspace-business-next"
                        variant="outline"
                        size="sm"
                        disabled={
                            !data || page * 20 >= data.count || query.isFetching
                        }
                        onClick={() => nextPage(page + 1)}
                    >
                        下一页
                    </Button>
                </div>
            </footer>
        </>
    )
}
