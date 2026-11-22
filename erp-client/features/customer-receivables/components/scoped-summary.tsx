"use client"

import * as React from "react"

import { FundsScopeBanner, FundsScopeSummaryBar } from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { ReceivableScopeListView } from "@/features/customer-receivables/api/scoped"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import { hasPermission } from "@/lib/permissions"

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

/** 范围汇总：紧凑口径条 + 统一无边框金额指标带（支持全部负责人 Popover 展开）。 */
export function ReceivableScopeSummary({ data }: ScopedSummaryProps) {
    const summary = data?.summary
    const allShares = React.useMemo(
        () => summary?.grouped ?? [],
        [summary?.grouped],
    )
    const missingNameIds = React.useMemo(
        () =>
            allShares
                .filter((share) => {
                    const item = share as ShareWithOptionalName
                    return (
                        !item.owner_user_name?.trim() &&
                        !item.owner_name?.trim()
                    )
                })
                .map((share) => share.owner_user_id)
                .filter(Boolean),
        [allShares],
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

    const groups = allShares.map((share) => ({
        id: share.owner_user_id,
        name: resolveGroupedOwnerName(
            share as ShareWithOptionalName,
            resolvedNames,
        ),
        amount: share.visible_share,
    }))

    return (
        <div className="flex min-w-0 flex-col">
            <FundsScopeBanner
                scopeSummary={data.scopeSummary}
                asOf={data.asOf}
                permissionLimited={limited}
                unassigned={summary?.unassigned}
            />
            {summary ? (
                <FundsScopeSummaryBar
                    wholeTotal={summary.whole_total}
                    unassigned={summary.unassigned}
                    groups={groups}
                    popoverIdPrefix="customer-receivables-scope-summary"
                />
            ) : null}
        </div>
    )
}
