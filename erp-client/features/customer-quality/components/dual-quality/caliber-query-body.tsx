"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import * as React from "react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { downloadQualityCsv } from "../../api/dual-caliber"
import type {
    CurrentQualityQuery,
    CurrentQualityView,
    HistoryQualityQuery,
    HistoryQualityView,
    QualityCaliber,
} from "../../dual-types"
import { toDualEmptyReason } from "../../dual-types"
import { useDualQualityExportMutation } from "../../hooks/dual-queries"
import type { PatchDual } from "../../lib/dual-filter-state"
import { Pager } from "./pager"
import { ScopeChangedPanel } from "./scope-changed-panel"
import { SummaryStrip } from "./summary-strip"

type CaliberView = CurrentQualityView | HistoryQualityView

/**
 * 当前负责与历史贡献共用的加载、空态、汇总、分页和导出。
 * 两个口径的筛选、表格和清除条件仍由各自面板传入。
 */
export function CaliberQueryBody({
    viewQuery,
    idPrefix,
    caliber,
    exportInput,
    page,
    pageSize,
    patchDual,
    pendingLabel,
    failureTitle,
    noScopeTitle,
    noScopeDescription,
    filteredTitle,
    noDataTitle,
    exportIdleLabel,
    onClearFilters,
    table,
}: {
    viewQuery: {
        isError: boolean
        isPending: boolean
        error: unknown
        data?: CaliberView
        refetch: () => unknown
    }
    idPrefix: string
    caliber: QualityCaliber
    exportInput: {
        current?: CurrentQualityQuery
        history?: HistoryQualityQuery
    }
    page: number
    pageSize: number
    patchDual: PatchDual
    pendingLabel: string
    failureTitle: string
    noScopeTitle: string
    noScopeDescription: string
    filteredTitle: string
    noDataTitle: string
    exportIdleLabel: string
    onClearFilters: () => void
    table: React.ReactNode
}) {
    const exportMutation = useDualQualityExportMutation(caliber)
    const [exportError, setExportError] = React.useState<string | null>(null)
    const [exportDone, setExportDone] = React.useState<string | null>(null)
    const data = viewQuery.data
    const emptyReason = toDualEmptyReason(data?.emptyReason)

    async function handleExport() {
        if (!data) return
        setExportError(null)
        setExportDone(null)
        try {
            const file = await exportMutation.mutateAsync(exportInput)
            downloadQualityCsv(file.csvContent, file.fileName)
            setExportDone(
                `已导出 ${file.rowCount} 行（${file.fileName}，${file.generatedAt} 生成）。`,
            )
        } catch (error) {
            setExportError(getErrorMessage(error, "导出失败，请重试。"))
        }
    }

    if (viewQuery.isError) {
        return isDataScopeChanged(viewQuery.error) ? (
            <ScopeChangedPanel
                idPrefix={idPrefix}
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
                title={failureTitle}
                error={viewQuery.error}
                onRetry={() => void viewQuery.refetch()}
            />
        )
    }
    if (viewQuery.isPending || !data) {
        return <p className="text-xs text-muted-foreground">{pendingLabel}</p>
    }
    if (emptyReason === "no-scope") {
        return (
            <BusinessEmptyState
                kind="no-scope"
                title={noScopeTitle}
                description={noScopeDescription}
            />
        )
    }
    return (
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
            />
            {emptyReason === "filtered-empty" || data.rows.total === 0 ? (
                <BusinessEmptyState
                    kind="filter"
                    title={filteredTitle}
                    description={`筛选：${data.filterSummary}`}
                    action={
                        <Button
                            id="customers-quality-dual-empty-clear"
                            type="button"
                            size="sm"
                            variant="secondary"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    }
                />
            ) : emptyReason === "no-data" ? (
                <BusinessEmptyState
                    kind="no-data"
                    title={noDataTitle}
                    description="可调整统计期间或数据范围后重查。"
                />
            ) : (
                table
            )}
            <Pager
                idPrefix={idPrefix}
                page={page}
                pageSize={pageSize}
                total={data.rows.total}
                scopeVersion={data.scopeVersion}
                patchDual={patchDual}
            />
            <div className="flex min-w-0 flex-wrap items-center gap-2">
                <PermissionGuard
                    permission={["customer:list", "sales_order:list"]}
                >
                    {data.canExport && (
                        <LoadingButton
                            id={`${idPrefix}-export`}
                            type="button"
                            loading={exportMutation.isPending}
                            variant="outline"
                            size="sm"
                            disabled={
                                data.rows.total === 0 ||
                                exportMutation.isPending
                            }
                            onClick={() => void handleExport()}
                        >
                            {exportMutation.isPending
                                ? "导出中…"
                                : exportIdleLabel}
                        </LoadingButton>
                    )}
                </PermissionGuard>
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
    )
}
