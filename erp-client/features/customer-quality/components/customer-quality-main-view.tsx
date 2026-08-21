"use client"

import * as React from "react"
import type { ColumnDef, PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import type {
    BusinessTag,
    BusinessTypeFilter,
    CustomerQualityExportJob,
    CustomerQualityPeriodPolicy,
    CustomerQualityRow,
    CustomerQualityView,
} from "../types"
import type { CustomerQualityPatch } from "../hooks/use-customer-quality-navigation-state"
import { BusinessTagDialog } from "./business-tag-dialog"
import { ChartFilterSummaryAlert } from "./chart-filter-summary-alert"
import { CustomerQualityCharts } from "./customer-quality-charts"
import { CustomerQualityCoveragePanels } from "./customer-quality-coverage-panels"
import { CustomerQualityDetailTable } from "./customer-quality-detail-table"
import { CustomerQualityExportProgress } from "./customer-quality-export-progress"
import { CustomerQualityMetricStrip } from "./customer-quality-metric-strip"
import { CustomerQualityPageAlerts } from "./customer-quality-page-alerts"
import { CustomerQualityPageHeader } from "./customer-quality-page-header"
import { CustomerQualityPeriodBar } from "./customer-quality-period-bar"

export function CustomerQualityMainView({
    data,
    viewError,
    onRetryView,
    toolbar,
    loading,
    hasActiveFilters,
    onClearFilters,
    refreshError,
    refreshing,
    onRefresh,
    exportPending,
    onExport,
    exportJob,
    resolvedFrom,
    resolvedTo,
    periodInvalid,
    presets,
    periodPreset,
    businessType,
    sort,
    onPresetSelect,
    patchUrl,
    resetPage,
    scaleDimension,
    profitDimension,
    natureDimension,
    chartDimension,
    chartCode,
    chartFilterSummary,
    setPagination,
    columns,
    pagination,
    onPaginationChange,
    tableSorting,
    onSortingChange,
    tableSectionRef,
    onFocusTable,
    tagDialog,
    onTagDialogOpenChange,
}: {
    data: CustomerQualityView | null
    viewError: unknown
    onRetryView: () => void
    toolbar: React.ReactNode
    loading: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
    refreshError: string | null
    refreshing: boolean
    onRefresh: () => void
    exportPending: boolean
    onExport: () => void
    exportJob: CustomerQualityExportJob | null
    resolvedFrom?: string
    resolvedTo?: string
    periodInvalid: boolean
    presets?: CustomerQualityPeriodPolicy["presets"]
    periodPreset?: string
    businessType?: BusinessTypeFilter
    sort: string
    onPresetSelect: (id: string) => void
    patchUrl: CustomerQualityPatch
    resetPage: () => void
    scaleDimension?: CustomerQualityView["dimensions"][number]
    profitDimension?: CustomerQualityView["dimensions"][number]
    natureDimension?: CustomerQualityView["dimensions"][number]
    chartDimension?: string
    chartCode?: string
    chartFilterSummary: {
        dimensionTitle: string
        itemLabel: string
        resultCount: number
    } | null
    setPagination: React.Dispatch<React.SetStateAction<PaginationState>>
    columns: ColumnDef<CustomerQualityRow>[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    tableSorting: SortingState
    onSortingChange: (next: SortingState) => void
    tableSectionRef: React.RefObject<HTMLDivElement | null>
    onFocusTable: () => void
    tagDialog: BusinessTag | null
    onTagDialogOpenChange: (open: boolean) => void
}) {
    const isVoucherOnly = businessType === "VOUCHER"
    const items = data?.customers.items ?? []
    const filteredTotal = data?.customers.filteredTotal ?? 0
    const total = data?.customers.total ?? 0
    const emptyKind = data?.emptyKind
    const filterSummary = data?.filterSummary ?? ""

    return (
        <PageScaffold>
            {data ? (
                <CustomerQualityPageHeader
                    freshness={data.freshness}
                    refreshing={refreshing}
                    period={data.period}
                    scopeLabel={data.scope.label}
                    onRefresh={onRefresh}
                    canExport={data.canExport}
                    filteredTotal={filteredTotal}
                    exportPending={exportPending}
                    onExport={onExport}
                />
            ) : (
                <PageHeader title="客户经营质量" />
            )}

            {data ? (
                <CustomerQualityPageAlerts
                    refreshError={refreshError}
                    freshness={data.freshness}
                />
            ) : null}

            <CustomerQualityPeriodBar
                resolvedFrom={resolvedFrom}
                resolvedTo={resolvedTo}
                periodInvalid={periodInvalid}
                presets={presets}
                periodPreset={periodPreset}
                sort={sort}
                onPresetSelect={onPresetSelect}
                patchUrl={patchUrl}
                resetPage={resetPage}
            />

            {data?.emptyKind === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无客户数据范围"
                    description="当前角色无客户数据范围，请申请权限。"
                />
            ) : (
                <>
                    {data ? (
                        <>
                            <CustomerQualityCoveragePanels
                                coverage={data.coverage}
                                isVoucherOnly={isVoucherOnly}
                                periodFrom={data.period.from}
                                periodTo={data.period.to}
                                onShowReviewedOnly={() =>
                                    patchUrl({ fundsReview: "reviewed_only" })
                                }
                            />

                            <CustomerQualityMetricStrip
                                metrics={data.metrics}
                                onFocusTable={onFocusTable}
                            />

                            <CustomerQualityCharts
                                scaleDimension={scaleDimension}
                                profitDimension={profitDimension}
                                natureDimension={natureDimension}
                                chartDimension={chartDimension}
                                chartCode={chartCode}
                                isVoucherOnly={isVoucherOnly}
                                patchUrl={patchUrl}
                                setPagination={setPagination}
                            />

                            {chartFilterSummary ? (
                                <ChartFilterSummaryAlert
                                    dimensionTitle={
                                        chartFilterSummary.dimensionTitle
                                    }
                                    itemLabel={chartFilterSummary.itemLabel}
                                    resultCount={chartFilterSummary.resultCount}
                                    onClear={() => {
                                        patchUrl({
                                            chartDimension: null,
                                            chartCode: null,
                                            scaleTag: null,
                                            profitTag: null,
                                            riskTag: null,
                                        })
                                        resetPage()
                                    }}
                                />
                            ) : null}
                        </>
                    ) : null}

                    {/* Customer detail table */}
                    <CustomerQualityDetailTable
                        sectionRef={tableSectionRef}
                        items={items}
                        filteredTotal={filteredTotal}
                        total={total}
                        columns={columns}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        sorting={tableSorting}
                        onSortingChange={onSortingChange}
                        emptyKind={emptyKind}
                        filterSummary={filterSummary}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={onClearFilters}
                        toolbar={toolbar}
                        viewError={viewError}
                        onRetryView={onRetryView}
                        loading={loading}
                    />
                </>
            )}

            {exportJob ? <CustomerQualityExportProgress job={exportJob} /> : null}

            <BusinessTagDialog
                tag={tagDialog}
                onOpenChange={onTagDialogOpenChange}
            />
        </PageScaffold>
    )
}
