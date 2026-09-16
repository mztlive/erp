"use client"

import * as React from "react"

import { FundsScopeBanner, MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ReceivableScopeListView } from "@/features/customer-receivables/api/scoped"
import { scopeText } from "@/lib/ui-text"

type ScopedSummaryProps = {
    data: ReceivableScopeListView | undefined
}

/** 范围汇总：人员分组只计匹配份额，未分配单列，整单受限时为空。 */
export function ReceivableScopeSummary({ data }: ScopedSummaryProps) {
    const summary = data?.summary
    if (!summary) return null
    const limited =
        summary.permission_limited ||
        (data?.receivables.some((row) => row.permission_limited) ?? false) ||
        (data?.receipts.some((row) => row.permission_limited) ?? false) ||
        (data?.invoices.some((row) => row.permission_limited) ?? false)
    return (
        <div className="min-w-0 space-y-3">
            <FundsScopeBanner
                scopeSummary={data?.scopeSummary}
                asOf={data?.asOf}
                permissionLimited={limited}
                unassigned={summary.unassigned}
            />
            <div className="grid min-w-0 gap-3 sm:grid-cols-3">
                <div className="min-w-0 rounded-lg border p-3">
                    <div className="text-xs text-muted-foreground">
                        整单合计
                    </div>
                    <div className="mt-1">
                        <MoneyValue
                            value={summary.whole_total}
                            unavailableReason={
                                summary.whole_total == null
                                    ? scopeText.wholeRestricted
                                    : undefined
                            }
                        />
                    </div>
                </div>
                <div className="min-w-0 rounded-lg border p-3">
                    <div className="text-xs text-muted-foreground">
                        {scopeText.unassignedShare}
                    </div>
                    <div className="mt-1">
                        <MoneyValue value={summary.unassigned} />
                    </div>
                </div>
                <div className="min-w-0 rounded-lg border p-3">
                    <div className="text-xs text-muted-foreground">
                        匹配分组（{summary.grouped.length}）
                    </div>
                    <ul className="mt-1 min-w-0 space-y-1">
                        {summary.grouped.length === 0 ? (
                            <li className="text-xs text-muted-foreground">
                                暂无匹配份额
                            </li>
                        ) : (
                            summary.grouped.slice(0, 5).map((share) => (
                                <li
                                    key={share.owner_user_id}
                                    className="flex min-w-0 items-baseline justify-between gap-2 text-xs"
                                >
                                    <span className="min-w-0 break-words">
                                        {share.owner_user_id}
                                    </span>
                                    <MoneyValue
                                        value={share.visible_share}
                                        className="shrink-0"
                                    />
                                </li>
                            ))
                        )}
                    </ul>
                </div>
            </div>
            {summary.grouped.length > 5 ? (
                <p className="text-xs text-muted-foreground">
                    仅展示前 5 个负责人分组，其余份额已计入整单与未分配口径。
                </p>
            ) : null}
        </div>
    )
}

/** 无范围 / 筛选无结果 / 请求失败三分态；失败分支由调用方渲染重试。 */
export function ReceivableScopeEmptyState({
    data,
    hasFilters,
    onClearFilters,
    clearId,
}: {
    data: ReceivableScopeListView | undefined
    hasFilters: boolean
    onClearFilters: () => void
    clearId: string
}) {
    if (!data) return null
    if (data.emptyReason === "no_scope" || !data.hasScope) {
        return (
            <Alert variant="warning">
                <AlertTitle>当前角色未配置客户往来范围</AlertTitle>
                <AlertDescription>
                    不得用 0 元假装无往来。请申请数据范围后再查询。
                </AlertDescription>
            </Alert>
        )
    }
    if (data.total === 0 && hasFilters) {
        return (
            <Alert variant="info">
                <AlertTitle>无匹配往来记录</AlertTitle>
                <AlertDescription>
                    无匹配记录，可清除筛选后重试。
                    <button
                        id={clearId}
                        type="button"
                        className="ml-2 underline"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </button>
                </AlertDescription>
            </Alert>
        )
    }
    return null
}
