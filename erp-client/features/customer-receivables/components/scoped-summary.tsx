"use client"

import * as React from "react"

import { FundsScopeBanner, MoneyValue } from "@/components/business"
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

function resolveGroupedOwnerName(
    share: ShareWithOptionalName,
    resolvedNames: ReadonlyMap<string, string>,
): string {
    const inline =
        share.owner_user_name?.trim() || share.owner_name?.trim() || ""
    if (inline) return inline
    return resolvedNames.get(share.owner_user_id) ?? "负责人（当前不可见）"
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
        account.isSuccess && canReadSales && missingNameIds.length > 0,
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

    if (!data) return null
    const limited =
        summary?.permission_limited ||
        data.receivables.some((row) => row.permission_limited) ||
        data.receipts.some((row) => row.permission_limited) ||
        data.invoices.some((row) => row.permission_limited)
    if (!summary && !data.scopeSummary && !limited) return null

    return (
        <div className="flex min-w-0 flex-col gap-3">
            <FundsScopeBanner
                scopeSummary={data.scopeSummary}
                asOf={data.asOf}
                permissionLimited={limited}
                unassigned={summary?.unassigned}
            />
            {summary ? (
                <dl className="grid min-w-0 gap-x-6 gap-y-2 border-b border-border/60 pb-3 text-body-compact sm:grid-cols-2 lg:grid-cols-[auto_auto_minmax(0,1fr)]">
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            整单合计
                        </dt>
                        <dd>
                            <MoneyValue
                                value={summary.whole_total}
                                display="exact"
                                unavailableReason={
                                    summary.whole_total == null
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            {scopeText.unassignedShare}
                        </dt>
                        <dd>
                            <MoneyValue
                                value={summary.unassigned}
                                display="exact"
                            />
                        </dd>
                    </div>
                    <div className="min-w-0 sm:col-span-2 lg:col-span-1">
                        <dt className="text-xs text-muted-foreground">
                            匹配分组（
                            <span className="num">
                                {summary.grouped.length}
                            </span>
                            ）
                        </dt>
                        <dd>
                            {topShares.length === 0 ? (
                                <span className="text-xs text-muted-foreground">
                                    暂无匹配份额
                                </span>
                            ) : (
                                <ul className="flex flex-wrap gap-x-4 gap-y-1">
                                    {topShares.map((share) => (
                                        <li
                                            key={share.owner_user_id}
                                            className="inline-flex min-w-0 flex-wrap items-baseline gap-x-1.5"
                                        >
                                            <span className="min-w-0 wrap-break-word text-muted-foreground">
                                                {resolveGroupedOwnerName(
                                                    share as ShareWithOptionalName,
                                                    resolvedNames,
                                                )}
                                            </span>
                                            <MoneyValue
                                                value={share.visible_share}
                                                display="exact"
                                            />
                                        </li>
                                    ))}
                                </ul>
                            )}
                            {summary.grouped.length > 5 ? (
                                <p className="text-xs text-muted-foreground">
                                    仅展示前 5
                                    个负责人分组，其余份额已计入整单与未分配口径。
                                </p>
                            ) : null}
                        </dd>
                    </div>
                </dl>
            ) : null}
        </div>
    )
}
