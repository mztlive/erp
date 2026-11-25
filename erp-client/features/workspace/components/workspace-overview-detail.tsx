"use client"

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useRouter } from "@/lib/navigation"
import {
    BusinessFailureState,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { fetchOverviewData } from "../api/overview"
import { overviewIdentity } from "../hooks/use-workspace-overview"
import type { OverviewMetric } from "../lib/overview"
import type { AccountProfile } from "@/features/auth/api"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { formatDateTimeInZone } from "@/lib/datetime"
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
    const router = useRouter()
    const metricKey = metric
        ? `${metric.key}:${JSON.stringify(metric.source)}`
        : ""
    const [paging, setPaging] = useState<{
        key: string
        page: number
        scopeVersion?: string
    }>({ key: "", page: 1 })
    const page = paging.key === metricKey ? paging.page : 1
    const scopeVersion =
        paging.key === metricKey ? paging.scopeVersion : undefined

    const query = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "overview-detail",
            profile ? overviewIdentity(profile) : "",
            metric?.source ?? null,
            page,
            scopeVersion,
        ],
        queryFn: () => fetchOverviewData(metric!.source, page, scopeVersion),
        enabled: Boolean(metric && profile),
        retry: false,
    })

    const data = query.isError ? undefined : query.data
    const nextPage = (value: number) => {
        setPaging({
            key: metricKey,
            page: value,
            scopeVersion: value === 1 ? undefined : data?.scopeVersion,
        })
    }

    return (
        <QuickPreviewSheet
            idPrefix="workspace-business-detail"
            open={open}
            size="preview"
            finalFocus={false}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            onOpenChangeComplete={(nextOpen) => {
                if (!nextOpen) onClosed()
            }}
            title={metric?.label || "业务明细"}
            description={metric?.detail}
            footer={
                metric && profile ? (
                    <div className="flex w-full items-center justify-between gap-3 text-xs text-muted-foreground">
                        <span className="num">
                            {data ? `共 ${data.count} 项` : "业务明细"}
                        </span>
                        <div className="flex items-center gap-2">
                            <Button
                                id="workspace-business-previous"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={page <= 1 || query.isFetching}
                                onClick={() => nextPage(page - 1)}
                            >
                                上一页
                            </Button>
                            <span className="num">第 {page} 页</span>
                            <Button
                                id="workspace-business-next"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    !data ||
                                    page * 20 >= data.count ||
                                    query.isFetching
                                }
                                onClick={() => nextPage(page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    </div>
                ) : null
            }
        >
            {metric && profile ? (
                query.isPending ? (
                    <p
                        role="status"
                        className="py-8 text-xs text-muted-foreground"
                    >
                        正在读取业务明细…
                    </p>
                ) : query.isError ? (
                    <BusinessFailureState
                        title="业务明细读取失败"
                        error={query.error}
                        onRetry={() => {
                            setPaging({
                                key: metricKey,
                                page: 1,
                                scopeVersion: undefined,
                            })
                            if (page === 1 && scopeVersion == null) {
                                void query.refetch()
                            }
                        }}
                    />
                ) : (
                    <div>
                        <p className="mb-2 text-body-compact font-medium">
                            {metric.label}：
                            {overviewValue(metric, data) == null ? (
                                "暂不可用"
                            ) : metric.field === "count" ? (
                                <span className="num">{data?.count} 项</span>
                            ) : (
                                <MoneyValue
                                    value={String(overviewValue(metric, data))}
                                />
                            )}
                        </p>
                        <p className="mb-4 text-xs text-muted-foreground">
                            {data?.scopeSummary || "当前授权范围"}
                            {data?.asOf ? (
                                <span className="num">
                                    {" "}
                                    · 数据时间{" "}
                                    {formatDateTimeInZone(
                                        data.asOf,
                                        "Asia/Shanghai",
                                    )}
                                </span>
                            ) : null}
                        </p>
                        {data?.note ? (
                            <p className="mb-4 rounded-lg bg-muted/40 p-3 text-body-compact">
                                {data.note}
                            </p>
                        ) : null}
                        {metric.field === "overdue" ? (
                            <p className="mb-4 text-xs text-muted-foreground">
                                逾期金额按每笔应收款的到期日计算。下方展示当前权限范围内的全部客户应收款余额（含未逾期），可点击进入客户往来查看逾期明细。
                            </p>
                        ) : null}
                        {!data?.rows.length ? (
                            <p className="py-10 text-center text-xs text-muted-foreground">
                                {data?.noScope
                                    ? "暂无数据查看权限"
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
                                                <p className="break-words text-body-compact font-medium">
                                                    {row.title}
                                                </p>
                                                <p className="mt-1 break-words text-xs text-muted-foreground">
                                                    {row.description}
                                                    {row.status
                                                        ? ` · ${row.status}`
                                                        : ""}
                                                </p>
                                            </div>
                                            {amount !== undefined ? (
                                                <span className="text-body-compact tabular-nums">
                                                    {amount == null ? (
                                                        "金额受限或暂不可用"
                                                    ) : (
                                                        <MoneyValue
                                                            value={amount}
                                                        />
                                                    )}
                                                </span>
                                            ) : null}
                                            {row.taskId || row.href ? (
                                                <Button
                                                    id={`workspace-business-row-${toAutomationIdSegment(row.id)}`}
                                                    type="button"
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
                                                    {row.taskId
                                                        ? "处理"
                                                        : "查看"}
                                                </Button>
                                            ) : null}
                                        </li>
                                    )
                                })}
                            </ul>
                        )}
                    </div>
                )
            ) : null}
        </QuickPreviewSheet>
    )
}
