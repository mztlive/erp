"use client"

import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { useActualProfitLossPage } from "@/features/actual-profit-loss/hooks/use-actual-profit-loss-page"
import { AnalysisBlockedPanel } from "@/features/actual-profit-loss/components/analysis-blocked-panel"
import { CostDetailSheet } from "@/features/actual-profit-loss/components/cost-detail-sheet"
import { CoverageAlert } from "@/features/actual-profit-loss/components/coverage-alert"
import { DataStatusAlerts } from "@/features/actual-profit-loss/components/data-status-alerts"
import { PeriodBasisPanel } from "@/features/actual-profit-loss/components/period-basis-panel"
import { ProfitLossChartsAndStageReference } from "@/features/actual-profit-loss/components/profit-loss-charts-and-stage-reference"
import { ProfitLossMetrics } from "@/features/actual-profit-loss/components/profit-loss-metrics"
import { ProfitLossPageHeader } from "@/features/actual-profit-loss/components/profit-loss-page-header"
import { ProfitLossRowsPanel } from "@/features/actual-profit-loss/components/profit-loss-rows-panel"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ActualProfitLossPage() {
    const page = useActualProfitLossPage()

    // —— 初载 / 配置加载 ——
    if (page.basisQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title={`实际经营盈亏（${SCOPE_LABEL}）`}
                    description="读取期间归属口径配置…"
                />
                <Skeleton className="h-16 w-full rounded-lg" />
                <Skeleton className="h-24 w-full rounded-lg" />
                <div className="grid gap-2">
                    {Array.from({ length: 5 }).map((_, i) => (
                        <Skeleton key={i} className="h-10 w-full rounded-lg" />
                    ))}
                </div>
            </PageScaffold>
        )
    }

    if (page.basisQuery.isError || !page.basisConfig) {
        return (
            <PageScaffold>
                <PageHeader title={`实际经营盈亏（${SCOPE_LABEL}）`} />
                <BusinessFailureState
                    error={page.basisQuery.error}
                    title="期间归属口径配置读取失败"
                    action={
                        <Button
                            id="actual-profit-loss-basis-retry"
                            type="button"
                            onClick={() => void page.basisQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <ProfitLossPageHeader
                hasData={page.data != null}
                projectedAt={page.data?.freshness.projectedAt}
                freshnessUi={page.freshnessUi}
                analysisReady={page.analysisReady}
                exportDisabled={
                    !page.analysisReady ||
                    !page.data ||
                    !page.data.fieldPermissions.canExport ||
                    page.data.rows.total === 0 ||
                    page.exportMutation.isPending
                }
                exportPending={page.exportMutation.isPending}
                onRefresh={() => void page.handleRefresh()}
                onExport={() => void page.handleExport()}
            />

            <PeriodBasisPanel
                presetRaw={page.periodPresetRaw}
                from={page.from}
                to={page.to}
                periodBasis={page.periodBasisUrl}
                basisConfig={page.basisConfig}
                periodBasisValid={page.periodBasisValid}
                patchUrl={page.patchUrl}
            />

            {/* 阻断：口径未配置且用户未显式选择 */}
            {page.analysisBlocked ? (
                <AnalysisBlockedPanel
                    basisConfig={page.basisConfig}
                    onSelectBasis={(code) =>
                        page.patchUrl({ periodBasis: code, page: null })
                    }
                />
            ) : null}

            {/* 分析主体：仅在口径就绪后；查询失败时筛选区保持挂载（失败态在明细表面内） */}
            {page.analysisReady ? (
                <>
                    {page.viewQuery.isPending && !page.data ? (
                        <>
                            <Skeleton className="h-20 w-full" />
                            <div className="grid gap-4 xl:grid-cols-5">
                                {Array.from({ length: 5 }).map((_, i) => (
                                    <Skeleton key={i} className="h-24 w-full" />
                                ))}
                            </div>
                            <Skeleton className="h-64 w-full" />
                        </>
                    ) : null}

                    {page.data ? (
                        <>
                            <DataStatusAlerts
                                data={page.data}
                                refreshFailed={page.refreshFailed}
                                exportFailed={page.exportFailed}
                                viewError={page.viewQuery.error}
                                isViewError={page.viewQuery.isError}
                                exportJob={page.exportJob}
                                onCloseExportJob={() => page.setExportJob(null)}
                            />
                            <CoverageAlert data={page.data} />
                            <ProfitLossMetrics data={page.data} />
                            <ProfitLossChartsAndStageReference
                                data={page.data}
                            />
                        </>
                    ) : null}

                    {page.data || page.viewQuery.isError ? (
                        <ProfitLossRowsPanel
                            data={page.data}
                            dimension={page.dimension}
                            coverage={page.coverage}
                            hasFilters={page.hasFilters}
                            searchInput={page.searchInput}
                            searchInputRef={page.searchInputRef}
                            onSearchInputChange={page.setSearchInput}
                            onApplyFilters={page.applyFilters}
                            onCoverageChange={page.handleCoverageChange}
                            panelOpen={page.filterPanelOpen}
                            setPanelOpen={page.setFilterPanelOpen}
                            hasStructuredFilters={page.hasStructuredFilters}
                            appliedChips={page.appliedChips}
                            onRemoveFilter={page.removeFilter}
                            onResetMoreFilters={page.resetMoreFilters}
                            onClearAllFilters={page.clearAllFilters}
                            onDimensionChange={page.handleDimensionChange}
                            benefitScenarioDraft={page.benefitScenarioDraft}
                            onBenefitScenarioDraftChange={
                                page.setBenefitScenarioDraft
                            }
                            fulfillmentModesDraft={page.fulfillmentModesDraft}
                            onFulfillmentModesDraftChange={
                                page.setFulfillmentModesDraft
                            }
                            costTypesDraft={page.costTypesDraft}
                            onCostTypesDraftChange={page.setCostTypesDraft}
                            benefitScenarioOptions={page.benefitScenarioOptions}
                            fulfillmentModeOptions={page.fulfillmentModeOptions}
                            costTypeOptions={page.costTypeOptions}
                            pageRows={page.pageRows}
                            columns={page.columns}
                            pagination={page.pagination}
                            onPaginationChange={page.setPagination}
                            sorting={page.tableSorting}
                            onSortingChange={page.handleTableSortingChange}
                            loading={
                                page.viewQuery.isFetching &&
                                !page.viewQuery.isPending
                            }
                            isError={page.viewQuery.isError}
                            error={page.viewQuery.error}
                            onRetry={() => void page.viewQuery.refetch()}
                        />
                    ) : null}
                </>
            ) : null}

            <CostDetailSheet
                open={page.costDetailRow != null}
                onOpenChange={(open) => {
                    if (!open) {
                        page.setCostDetailRow(null)
                        page.setSelectedCostEntryId(null)
                    }
                }}
                costDetailRow={page.costDetailRow}
                costEntries={page.costEntriesQuery}
                selectedCostEntryId={page.selectedCostEntryId}
                selectedEntry={page.selectedEntry}
                onSelectEntry={page.setSelectedCostEntryId}
            />
        </PageScaffold>
    )
}
