"use client"

import * as React from "react"

import {
    BusinessFailureState,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import {
    CalendarRangeIcon,
    DownloadIcon,
    InfoIcon,
    RefreshCwIcon,
} from "lucide-react"

import { hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { CardBusinessBasisSheet } from "@/features/card-business-analytics/components/card-business-basis-sheet"
import { CardBusinessCharts } from "@/features/card-business-analytics/components/card-business-charts"
import { CardBusinessCostCoverage } from "@/features/card-business-analytics/components/card-business-cost-coverage"
import { CardBusinessDrillTable } from "@/features/card-business-analytics/components/card-business-drill-table"
import { CardBusinessExportSheet } from "@/features/card-business-analytics/components/card-business-export-sheet"
import { CardBusinessFilterBar } from "@/features/card-business-analytics/components/card-business-filter-bar"
import { CardBusinessFilterSummary } from "@/features/card-business-analytics/components/card-business-filter-summary"
import { CardBusinessFinalProfitAlert } from "@/features/card-business-analytics/components/card-business-final-profit-alert"
import { CardBusinessHeaderMetadata } from "@/features/card-business-analytics/components/card-business-header-metadata"
import { CardBusinessMetricStrip } from "@/features/card-business-analytics/components/card-business-metric-strip"
import { CardBusinessPeriodConfig } from "@/features/card-business-analytics/components/card-business-period-config"
import { CardBusinessStatusAlerts } from "@/features/card-business-analytics/components/card-business-status-alerts"
import {
    useCardBusinessAnalyticsQuery,
    useDateBasisConfigQuery,
} from "@/features/card-business-analytics/hooks/queries"
import { useCardBusinessColumns } from "@/features/card-business-analytics/hooks/use-card-business-columns"
import { useCardBusinessExport } from "@/features/card-business-analytics/hooks/use-card-business-export"
import { useCardBusinessPageState } from "@/features/card-business-analytics/hooks/use-card-business-page-state"
import { useCardBusinessRefresh } from "@/features/card-business-analytics/hooks/use-card-business-refresh"
import type { PeriodPreset } from "@/features/card-business-analytics/types"

export function CardBusinessAnalyticsPage() {
    const accountProfile = useAccountProfileQuery()
    const canReadAllCustomers = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )

    const basisQuery = useDateBasisConfigQuery()
    const basisConfig = basisQuery.data

    const state = useCardBusinessPageState(basisConfig, basisQuery.isSuccess)

    const viewQuery = useCardBusinessAnalyticsQuery(
        state.analysisQuery,
        state.analysisReady,
    )
    const data = viewQuery.data
    const columns = useCardBusinessColumns(data)

    const {
        exportJob,
        setExportJob,
        exportPreviewOpen,
        setExportPreviewOpen,
        handleExportConfirm,
        isExporting,
    } = useCardBusinessExport({
        data,
        analysisQuery: state.analysisQuery,
    })

    const { refreshing, refreshFailed, handleRefresh } = useCardBusinessRefresh(
        () => viewQuery.refetch(),
    )

    const [basisSheetOpen, setBasisSheetOpen] = React.useState(false)

    const dateBasisOptions = (basisConfig?.allowedDateBases ?? []).map((b) => ({
        value: b.code,
        label: b.label,
    }))

    const hasActiveFilters = Boolean(
        state.customerId ||
        state.salesOrderId ||
        (state.costBasis && state.costBasis.length > 0) ||
        state.expiryState !== "all" ||
        state.coverage !== "all",
    )

    function clearFilters() {
        state.patchUrl({
            customerId: null,
            salesOrderId: null,
            costBasis: null,
            expiryState: null,
            coverage: null,
        })
    }

    // —— Loading shells ——
    if (basisQuery.isPending) {
        return (
            <PageScaffold>
                <Skeleton className="h-10 w-64 rounded-lg" />
                <Skeleton className="h-24 w-full rounded-lg" />
                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                    {Array.from({ length: 8 }).map((_, i) => (
                        <Skeleton key={i} className="h-20 rounded-lg" />
                    ))}
                </div>
                <Skeleton className="h-64 w-full rounded-lg" />
            </PageScaffold>
        )
    }

    if (basisQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={basisQuery.error}
                    title="日期口径配置加载失败"
                    action={
                        <Button
                            id="card-contracts-analytics-basis-retry"
                            type="button"
                            onClick={() => void basisQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    // —— Q2 期间与日期口径选择态 ——
    if (state.analysisBlocked) {
        return (
            <PageScaffold>
                <PageHeader
                    title="卡券消费台账与经营分析"
                    description="系统尚未配置默认日期口径。请显式选择期间与日期口径后再开始分析。"
                />
                <Alert variant="warning">
                    <CalendarRangeIcon aria-hidden="true" />
                    <AlertTitle>请选择期间与日期口径</AlertTitle>
                    <AlertDescription>
                        选择完整的期间与日期口径后才会发起查询；该选择将作用于全部指标与图表。
                    </AlertDescription>
                </Alert>
                <CardBusinessPeriodConfig
                    explicitFrom={state.explicitFrom}
                    explicitTo={state.explicitTo}
                    explicitDateBasis={state.explicitDateBasis}
                    dateBasisOptions={dateBasisOptions}
                    onFromChange={state.setExplicitFrom}
                    onToChange={state.setExplicitTo}
                    onDateBasisChange={state.setExplicitDateBasis}
                    onApply={state.applyExplicitPeriod}
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title="卡券消费台账与经营分析"
                metadata={
                    data ? (
                        <CardBusinessHeaderMetadata
                            freshness={data.freshness}
                            refreshing={refreshing}
                            refreshFailed={refreshFailed}
                        />
                    ) : null
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "basis",
                                id: "card-contracts-analytics-header-basis",
                                label: "口径说明",
                                icon: InfoIcon,
                                variant: "outline",
                                onClick: () => setBasisSheetOpen(true),
                            },
                            {
                                actionKey: "refresh",
                                id: "card-contracts-analytics-header-refresh",
                                label: refreshing ? "刷新中" : "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                className:
                                    "text-muted-foreground hover:text-foreground",
                                disabled: !data || refreshing,
                                onClick: () => {
                                    void handleRefresh()
                                },
                            },
                            {
                                actionKey: "export",
                                id: "card-contracts-analytics-header-export",
                                label: "导出",
                                icon: DownloadIcon,
                                disabled: !data?.fieldPermissions.canExport,
                                onClick: () => setExportPreviewOpen(true),
                            },
                        ]}
                    />
                }
            />

            {/* Filter bar */}
            <CardBusinessFilterBar
                periodPresetValue={state.periodPresetValue}
                from={state.from}
                to={state.to}
                dateBasis={state.dateBasis}
                dateBasisOptions={dateBasisOptions}
                customerId={state.customerId}
                salesOrderId={state.salesOrderId}
                canReadAllCustomers={canReadAllCustomers}
                costBasisValue={state.costBasis?.join(",") ?? ""}
                expiryState={state.expiryState}
                coverage={state.coverage}
                dimension={state.dimension}
                hasActiveFilters={hasActiveFilters}
                onPresetChange={(preset: PeriodPreset) =>
                    state.applyPreset(preset)
                }
                onFromChange={(next) =>
                    state.patchUrl({ from: next, periodPreset: null })
                }
                onToChange={(next) =>
                    state.patchUrl({ to: next, periodPreset: null })
                }
                onDateBasisChange={(v) =>
                    state.patchUrl({ dateBasis: v ?? state.dateBasis })
                }
                onCustomerChange={(id) =>
                    state.patchUrl({ customerId: id || null })
                }
                onSalesOrderChange={(id) =>
                    state.patchUrl({ salesOrderId: id || null })
                }
                onCostBasisChange={(v) =>
                    state.patchUrl({ costBasis: v || null })
                }
                onExpiryChange={(v) =>
                    state.patchUrl({ expiryState: (v ?? "all") || null })
                }
                onCoverageChange={(v) =>
                    state.patchUrl({
                        coverage: v && v !== "all" ? v : null,
                    })
                }
                onDimensionChange={(v) =>
                    state.patchUrl({ dimension: v ?? state.dimension })
                }
                onClearFilters={clearFilters}
            />

            {viewQuery.isPending && !data ? (
                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                    {Array.from({ length: 8 }).map((_, i) => (
                        <Skeleton key={i} className="h-20 rounded-lg" />
                    ))}
                </div>
            ) : null}

            {viewQuery.isError && !data ? (
                <BusinessFailureState
                    error={viewQuery.error}
                    title="卡券经营数据加载失败"
                    action={
                        <Button
                            id="card-contracts-analytics-view-retry"
                            type="button"
                            onClick={() => void viewQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            ) : null}

            {data ? (
                <>
                    <CardBusinessStatusAlerts
                        data={data}
                        viewError={viewQuery.error}
                        refreshFailed={refreshFailed}
                        exportJob={exportJob}
                        onCloseExportJob={() => setExportJob(null)}
                    />

                    <CardBusinessFilterSummary
                        filterSummary={data.filterSummary}
                        wechatExcludedNote={data.wechatExcludedNote}
                    />

                    <CardBusinessCostCoverage data={data} />

                    <CardBusinessMetricStrip
                        metrics={data.metrics}
                        profitReferenceOnly={data.coverage.profitReferenceOnly}
                    />

                    <CardBusinessFinalProfitAlert
                        data={data}
                        onSwitchToExpiry={() =>
                            state.patchUrl({
                                dateBasis: "expiry",
                                expiryState: "expired",
                            })
                        }
                    />

                    <CardBusinessCharts data={data} />

                    <CardBusinessDrillTable
                        data={data}
                        columns={columns}
                        pagination={state.pagination}
                        tableSorting={state.tableSorting}
                        onPaginationChange={state.handlePaginationChange}
                        onSortingChange={state.handleTableSortingChange}
                        onClearFilters={clearFilters}
                    />
                </>
            ) : null}

            {/* 口径说明 Sheet */}
            <CardBusinessBasisSheet
                open={basisSheetOpen}
                onOpenChange={setBasisSheetOpen}
            />

            {/* 导出预览：口径/筛选/水位/覆盖率 disclaimer */}
            <CardBusinessExportSheet
                open={exportPreviewOpen}
                onOpenChange={setExportPreviewOpen}
                data={data}
                isExporting={isExporting}
                onConfirmExport={() => void handleExportConfirm()}
            />
        </PageScaffold>
    )
}
