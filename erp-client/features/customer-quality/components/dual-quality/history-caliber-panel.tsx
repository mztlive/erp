"use client"

import * as React from "react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { useHistoricalDirectory } from "@/lib/historical-directory"
import { downloadQualityCsv } from "../../api/dual-caliber"
import type { HistoryQualityView } from "../../dual-types"
import { toDualEmptyReason } from "../../dual-types"
import {
    useHistoryQualityQuery,
    useDualQualityExportMutation,
} from "../../hooks/dual-queries"
import { useHistoryQualityFilters } from "../../hooks/use-history-quality-filters"
import { useScopeVersionWriteBack } from "../../hooks/use-scope-version-write-back"
import type { PatchDual } from "../../lib/dual-filter-state"
import { HistoryCaliberFilters } from "./history-caliber-filters"
import { HistoryRowsTable } from "./history-rows-table"
import { Pager } from "./pager"
import { ScopeChangedPanel } from "./scope-changed-panel"
import { SummaryStrip } from "./summary-strip"

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
    const exportMutation = useDualQualityExportMutation("history")
    const [exportError, setExportError] = React.useState<string | null>(null)
    const [exportDone, setExportDone] = React.useState<string | null>(null)

    const data: HistoryQualityView | undefined = viewQuery.data
    const emptyReason = toDualEmptyReason(data?.emptyReason)
    async function handleExport() {
        if (!data) return
        setExportError(null)
        setExportDone(null)
        try {
            const file = await exportMutation.mutateAsync({ history: query })
            downloadQualityCsv(file.csvContent, file.fileName)
            setExportDone(
                `已导出 ${file.rowCount} 行（${file.fileName}，${file.generatedAt} 生成，版本已绑定）。`,
            )
        } catch (error) {
            setExportError(getErrorMessage(error, "导出失败，请重试。"))
        }
    }

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

            {viewQuery.isError ? (
                isDataScopeChanged(viewQuery.error) ? (
                    <ScopeChangedPanel
                        idPrefix="customers-quality-dual-history"
                        error={viewQuery.error}
                        onRefreshFromFirst={() =>
                            patchDual({
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                        onRetry={() => void viewQuery.refetch()}
                    />
                ) : (
                    <BusinessFailureState
                        title="历史贡献口径加载失败"
                        error={viewQuery.error}
                        onRetry={() => void viewQuery.refetch()}
                    />
                )
            ) : viewQuery.isPending || !data ? (
                <p className="text-sm text-muted-foreground">
                    正在加载历史贡献口径…
                </p>
            ) : emptyReason === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无销售单数据范围"
                    description="当前角色无可查看的销售单范围，请申请权限。当前负责口径不受此影响，可切换查看。"
                />
            ) : (
                <>
                    <SummaryStrip
                        scopeSummary={data.scopeSummary}
                        ownershipBasis={data.ownershipBasis}
                        asOf={data.asOf}
                        filterSummary={data.filterSummary}
                        objectCount={data.totals.objectCount}
                        orderCount={data.totals.orderCount}
                        grossTotal={data.totals.grossTotal}
                        unpricedCount={data.totals.unpricedCount}
                        policyVersion={data.policyVersion}
                        organizationVersion={data.organizationVersion}
                        scopeVersion={data.scopeVersion}
                    />
                    {emptyReason === "filtered-empty" ||
                    data.rows.total === 0 ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无历史贡献结果"
                            description={`筛选：${data.filterSummary}`}
                            action={
                                <Button
                                    id="customers-quality-dual-empty-clear"
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
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
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : emptyReason === "no-data" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="期间内无授权历史订单"
                            description="可调整统计期间或数据范围后重查。"
                        />
                    ) : (
                        <HistoryRowsTable
                            items={data.rows.items}
                            dimension={dimension}
                            patchDual={patchDual}
                        />
                    )}
                    <Pager
                        idPrefix="customers-quality-dual-history"
                        page={page}
                        pageSize={pageSize}
                        total={data.rows.total}
                        scopeVersion={data.scopeVersion}
                        patchDual={patchDual}
                    />
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        <LoadingButton
                            id="customers-quality-dual-history-export"
                            type="button"
                            loading={exportMutation.isPending}
                            variant="outline"
                            size="sm"
                            disabled={
                                !data.canExport ||
                                data.rows.total === 0 ||
                                exportMutation.isPending
                            }
                            onClick={() => void handleExport()}
                        >
                            {exportMutation.isPending
                                ? "导出中…"
                                : "导出历史口径 CSV"}
                        </LoadingButton>
                        {exportError ? (
                            <span className="min-w-0 break-all text-[13px] text-destructive">
                                {exportError}
                            </span>
                        ) : null}
                        {exportDone ? (
                            <span className="min-w-0 break-all text-[13px] text-muted-foreground">
                                {exportDone}
                            </span>
                        ) : null}
                    </div>
                </>
            )}
        </div>
    )
}
