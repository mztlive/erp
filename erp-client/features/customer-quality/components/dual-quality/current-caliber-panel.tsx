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
                按销售单当前负责销售和业务部门汇总，同一客户的不同业务分别归属；如需按签单时归属统计，请切换至历史贡献口径。
            </p>

            <CurrentCaliberFilters
                filters={filters}
                patchDual={patchDual}
                items={viewQuery.data?.rows.items}
            />

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
                noScopeTitle="暂无可查看的客户数据"
                noScopeDescription="您当前尚未配置当前负责客户的数据查看范围，请联系管理员开通权限；或切换查看历史贡献口径。"
                filteredTitle="当前筛选无客户结果"
                noDataTitle="当前期间暂无客户经营数据"
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
