"use client"

import * as React from "react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { downloadQualityCsv } from "../../api/dual-caliber"
import type { CurrentQualityView } from "../../dual-types"
import { toDualEmptyReason } from "../../dual-types"
import {
    useCurrentQualityQuery,
    useDualQualityExportMutation,
} from "../../hooks/dual-queries"
import { useCurrentQualityFilters } from "../../hooks/use-current-quality-filters"
import { useScopeVersionWriteBack } from "../../hooks/use-scope-version-write-back"
import type { PatchDual } from "../../lib/dual-filter-state"
import { CurrentCaliberFilters } from "./current-caliber-filters"
import { CurrentRowsTable } from "./current-rows-table"
import { Pager } from "./pager"
import { ScopeChangedPanel } from "./scope-changed-panel"
import { SummaryStrip } from "./summary-strip"

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
    const exportMutation = useDualQualityExportMutation("current")
    const [exportError, setExportError] = React.useState<string | null>(null)
    const [exportDone, setExportDone] = React.useState<string | null>(null)

    const data: CurrentQualityView | undefined = viewQuery.data
    const emptyReason = toDualEmptyReason(data?.emptyReason)
    async function handleExport() {
        if (!data) return
        setExportError(null)
        setExportDone(null)
        try {
            const file = await exportMutation.mutateAsync({ current: query })
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
                当前负责口径：按客户现任主责与主责所属组织分组汇总；只看现任归属，不读取历史冻结快照。
            </p>

            <CurrentCaliberFilters filters={filters} patchDual={patchDual} />

            {viewQuery.isError ? (
                isDataScopeChanged(viewQuery.error) ? (
                    <ScopeChangedPanel
                        idPrefix="customers-quality-dual-current"
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
                        title="当前负责口径加载失败"
                        error={viewQuery.error}
                        onRetry={() => void viewQuery.refetch()}
                    />
                )
            ) : viewQuery.isPending || !data ? (
                <p className="text-sm text-muted-foreground">
                    正在加载当前负责口径…
                </p>
            ) : emptyReason === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无客户数据范围"
                    description="当前角色无客户数据范围，请申请权限。历史贡献口径不受此影响，可切换查看。"
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
                            title="当前筛选无客户结果"
                            description={`筛选：${data.filterSummary}`}
                            action={
                                <Button
                                    id="customers-quality-dual-empty-clear"
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
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
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : emptyReason === "no-data" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="期间内无授权经营记录"
                            description="可调整统计期间或数据范围后重查。"
                        />
                    ) : (
                        <CurrentRowsTable
                            items={data.rows.items}
                            dimension={dimension}
                            patchDual={patchDual}
                        />
                    )}
                    <Pager
                        idPrefix="customers-quality-dual-current"
                        page={page}
                        pageSize={pageSize}
                        total={data.rows.total}
                        scopeVersion={data.scopeVersion}
                        patchDual={patchDual}
                    />
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        <LoadingButton
                            id="customers-quality-dual-current-export"
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
                                : "导出当前口径 CSV"}
                        </LoadingButton>
                        {exportError ? (
                            <span className="min-w-0 break-all text-body-compact text-destructive">
                                {exportError}
                            </span>
                        ) : null}
                        {exportDone ? (
                            <span className="min-w-0 break-all text-body-compact text-muted-foreground">
                                {exportDone}
                            </span>
                        ) : null}
                    </div>
                </>
            )}
        </div>
    )
}
