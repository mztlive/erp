"use client"

import { useHistoricalDirectory } from "@/lib/historical-directory"
import { useHistoryQualityQuery } from "../../hooks/dual-queries"
import { useHistoryQualityFilters } from "../../hooks/use-history-quality-filters"
import { useScopeVersionWriteBack } from "../../hooks/use-scope-version-write-back"
import type { PatchDual } from "../../lib/dual-filter-state"
import { CaliberQueryBody } from "./caliber-query-body"
import { HistoryCaliberFilters } from "./history-caliber-filters"
import { HistoryRowsTable } from "./history-rows-table"

export function HistoryCaliberPanel({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const filters = useHistoryQualityFilters({ from, to, patchDual })
    const { searchParams, query, dimension, page, pageSize } = filters
    const viewQuery = useHistoryQualityQuery(query)
    const directoryQuery = useHistoricalDirectory(
        "/admin/customer-quality/history/directory",
        query,
    )
    useScopeVersionWriteBack(
        viewQuery.data?.scopeVersion,
        page,
        patchDual,
        searchParams,
    )

    return (
        <div className="flex min-w-0 flex-col gap-3">
            <p className="text-xs text-muted-foreground">
                历史贡献口径：按销售单首次生效时冻结的负责人与组织祖先路径分组汇总；人员调岗、客户换任不改写历史，永不用现任负责人回填。
            </p>

            <HistoryCaliberFilters
                filters={filters}
                patchDual={patchDual}
                directoryQuery={directoryQuery}
            />

            <CaliberQueryBody
                viewQuery={viewQuery}
                idPrefix="customers-quality-dual-history"
                caliber="history"
                exportInput={{ history: query }}
                page={page}
                pageSize={pageSize}
                patchDual={patchDual}
                pendingLabel="正在加载历史贡献口径…"
                failureTitle="历史贡献口径加载失败"
                noScopeTitle="当前角色无销售单数据范围"
                noScopeDescription="当前角色无可查看的销售单范围，请申请权限。当前负责口径不受此影响，可切换查看。"
                filteredTitle="当前筛选无历史贡献结果"
                noDataTitle="期间内无授权历史订单"
                exportIdleLabel="导出历史口径 CSV"
                onClearFilters={() =>
                    patchDual({
                        attributionUserIds: null,
                        attributionOrgUnitIds: null,
                        attributionGroup: null,
                        dualCustomerId: null,
                        dualQ: null,
                        scopeVersion: null,
                        dualPage: null,
                    })
                }
                table={
                    <HistoryRowsTable
                        items={viewQuery.data?.rows.items ?? []}
                        dimension={dimension}
                        patchDual={patchDual}
                    />
                }
            />
        </div>
    )
}
