"use client"

import * as React from "react"

import { FundsScopeBanner, MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { ReceivableScopeListView } from "@/features/customer-receivables/api/scoped"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import { hasPermission } from "@/lib/permissions"
import { scopeText } from "@/lib/ui-text"

type ScopedSummaryProps = {
    data: ReceivableScopeListView | undefined
}

type ShareWithOptionalName = {
    owner_user_id: string
    owner_user_name?: string | null
    owner_name?: string | null
}

function isInternalIdentifier(value: string): boolean {
    const trimmed = value.trim()
    return (
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            trimmed,
        ) || /^[a-z]{2,6}_[a-z0-9_-]{6,}$/i.test(trimmed)
    )
}

function resolveGroupedOwnerName(
    share: ShareWithOptionalName,
    resolvedNames: ReadonlyMap<string, string>,
): string {
    const inline =
        share.owner_user_name?.trim() || share.owner_name?.trim() || ""
    if (inline) return inline
    const fromDirectory = resolvedNames.get(share.owner_user_id)
    if (fromDirectory) return fromDirectory
    const rawId = share.owner_user_id.trim()
    if (!rawId || isInternalIdentifier(rawId)) return "负责人（当前不可见）"
    return rawId
}

/** 范围汇总：人员分组只计匹配份额，未分配单列，整单受限时为空。 */
export function ReceivableScopeSummary({ data }: ScopedSummaryProps) {
    const summary = data?.summary
    const topShares = React.useMemo(
        () => summary?.grouped.slice(0, 5) ?? [],
        [summary?.grouped],
    )
    const missingNameIds = React.useMemo(
        () =>
            topShares
                .filter((share) => {
                    const item = share as ShareWithOptionalName
                    return (
                        !item.owner_user_name?.trim() &&
                        !item.owner_name?.trim()
                    )
                })
                .map((share) => share.owner_user_id)
                .filter(Boolean),
        [topShares],
    )
    const account = useAccountProfileQuery()
    const canReadSales = hasPermission(
        account.data?.permissions,
        "sales_person:list",
    )
    const selectedPeople = usePersonDirectorySelected(
        "sales",
        missingNameIds,
        canReadSales && missingNameIds.length > 0,
    )
    const resolvedNames = React.useMemo(() => {
        const map = new Map<string, string>()
        if (selectedPeople.isSuccess && selectedPeople.data) {
            for (const item of selectedPeople.data.items) {
                map.set(item.id, item.name.trim() || personDirectoryLabel(item))
            }
        }
        return map
    }, [selectedPeople.data, selectedPeople.isSuccess])

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
                            topShares.map((share) => (
                                <li
                                    key={share.owner_user_id}
                                    className="flex min-w-0 items-baseline justify-between gap-2 text-xs"
                                >
                                    <span className="min-w-0 break-words">
                                        {resolveGroupedOwnerName(
                                            share as ShareWithOptionalName,
                                            resolvedNames,
                                        )}
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
                <AlertTitle>暂无可查看的客户往来数据</AlertTitle>
                <AlertDescription>
                    您当前尚未配置客户往来的数据查看范围，请联系管理员开通相应权限后再查询。
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
