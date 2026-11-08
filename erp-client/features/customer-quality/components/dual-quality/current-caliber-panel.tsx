"use client"

import { useCurrentQualityQuery } from "../../hooks/dual-queries"
import { useCurrentQualityFilters } from "../../hooks/use-current-quality-filters"
import { useScopeVersionWriteBack } from "../../hooks/use-scope-version-write-back"
import type { PatchDual } from "../../lib/dual-filter-state"
import { CaliberQueryBody } from "./caliber-query-body"
import { CurrentCaliberFilters } from "./current-caliber-filters"
import { CurrentRowsTable } from "./current-rows-table"

export function CurrentCaliberPanel({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const filters = useCurrentQualityFilters({ from, to, patchDual })
    const { searchParams, query, dimension, page, pageSize } = filters
    const viewQuery = useCurrentQualityQuery(query)
    useScopeVersionWriteBack(
        viewQuery.data?.scopeVersion,
        page,
        patchDual,
        searchParams,
    )

    return (
        <div className="flex min-w-0 flex-col gap-3">
            <p className="text-xs text-muted-foreground">
                当前负责口径：按销售单当前负责销售和业务组织汇总，同一客户的不同业务分别归属；历史贡献使用生效时的归属。
            </p>

            <CurrentCaliberFilters filters={filters} patchDual={patchDual} />

            <CaliberQueryBody
                viewQuery={viewQuery}
                idPrefix="customers-quality-dual-current"
                caliber="current"
                exportInput={{ current: query }}
                page={page}
                pageSize={pageSize}
                patchDual={patchDual}
                pendingLabel="正在加载当前负责口径…"
                failureTitle="当前负责口径加载失败"
                noScopeTitle="当前角色无客户数据范围"
                noScopeDescription="当前角色无客户数据范围，请申请权限。历史贡献口径不受此影响，可切换查看。"
                filteredTitle="当前筛选无客户结果"
                noDataTitle="期间内无授权经营记录"
                exportIdleLabel="导出当前口径 CSV"
                onClearFilters={() =>
                    patchDual({
                        ownerUserIds: null,
                        orgUnitIds: null,
                        includeDescendants: null,
                        ownerGroup: null,
                        dualCustomerId: null,
                        dualQ: null,
                        scopeVersion: null,
                        dualPage: null,
                    })
                }
                table={
                    <CurrentRowsTable
                        items={viewQuery.data?.rows.items ?? []}
                        dimension={dimension}
                        patchDual={patchDual}
                    />
                }
            />
        </div>
    )
}
