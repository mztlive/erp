"use client"

import * as React from "react"
import type { ColumnDef, PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    PageScaffold,
} from "@/components/business"
import type {
    BusinessTag,
    BusinessTypeFilter,
    CustomerQualityExportJob,
    CustomerQualityPeriodPolicy,
    CustomerQualityRow,
    CustomerQualityView,
    FundsReviewFilter,
} from "../types"
import type { CustomerQualityPatch } from "../hooks/use-customer-quality-navigation-state"
import { BusinessTagDialog } from "./business-tag-dialog"
import { ChartFilterSummaryAlert } from "./chart-filter-summary-alert"
import { CustomerQualityCharts } from "./customer-quality-charts"
import { CustomerQualityCoveragePanels } from "./customer-quality-coverage-panels"
import { CustomerQualityDetailTable } from "./customer-quality-detail-table"
import { CustomerQualityExportProgress } from "./customer-quality-export-progress"
import { CustomerQualityFilterCard } from "./customer-quality-filter-card"
import { CustomerQualityMetricStrip } from "./customer-quality-metric-strip"
import { CustomerQualityPageAlerts } from "./customer-quality-page-alerts"
import { CustomerQualityPageHeader } from "./customer-quality-page-header"

export function CustomerQualityMainView({
    data,
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
    fundsReview,
    businessType,
    sort,
    searchInput,
    searchInputRef,
    onSearchInputChange,
    customerId,
    chipCustomerName,
    showClearFilters,
    onClearFilters,
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
    onClearTableFilters,
    tagDialog,
    onTagDialogOpenChange,
}: {
    data: CustomerQualityView
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
    fundsReview: FundsReviewFilter
    businessType?: BusinessTypeFilter
    sort: string
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    customerId?: string
    chipCustomerName?: string
    showClearFilters: boolean
    onClearFilters: () => void
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
    onClearTableFilters: () => void
    tagDialog: BusinessTag | null
    onTagDialogOpenChange: (open: boolean) => void
}) {
    const isVoucherOnly = businessType === "VOUCHER"

    return (
        <PageScaffold>
            <CustomerQualityPageHeader
                freshness={data.freshness}
                refreshing={refreshing}
                period={data.period}
                scopeLabel={data.scope.label}
                onRefresh={onRefresh}
                canExport={data.canExport}
                filteredTotal={data.customers.filteredTotal}
                exportPending={exportPending}
                onExport={onExport}
            />

            <CustomerQualityPageAlerts
                refreshError={refreshError}
                freshness={data.freshness}
            />

            {/* Filters */}
            <CustomerQualityFilterCard
                resolvedFrom={resolvedFrom}
                resolvedTo={resolvedTo}
                periodInvalid={periodInvalid}
                presets={presets}
                periodPreset={periodPreset}
                fundsReview={fundsReview}
                businessType={businessType}
                sort={sort}
                searchInput={searchInput}
                searchInputRef={searchInputRef}
                onSearchInputChange={onSearchInputChange}
                customerId={customerId}
                chipCustomerName={chipCustomerName}
                showClearFilters={showClearFilters}
                onClearFilters={onClearFilters}
                onPresetSelect={onPresetSelect}
                patchUrl={patchUrl}
                resetPage={resetPage}
                filterSummary={data.filterSummary}
                filteredTotal={data.customers.filteredTotal}
                total={data.customers.total}
            />

            {data.emptyKind === "no-scope" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无客户数据范围"
                    description="当前角色无客户数据范围，请申请权限。"
                />
            ) : (
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

                    {/* Metrics */}
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
                            dimensionTitle={chartFilterSummary.dimensionTitle}
                            itemLabel={chartFilterSummary.itemLabel}
                            resultCount={chartFilterSummary.resultCount}
                            onClear={() => {
                                patchUrl({
                                    chartDimension: null,
                                    chartCode: null,
                                    scaleTag: null,
                                    profitTag: null,
                                })
                                resetPage()
                            }}
                        />
                    ) : null}

                    {/* Customer detail table */}
                    <CustomerQualityDetailTable
                        sectionRef={tableSectionRef}
                        items={data.customers.items}
                        filteredTotal={data.customers.filteredTotal}
                        columns={columns}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        sorting={tableSorting}
                        onSortingChange={onSortingChange}
                        emptyKind={data.emptyKind}
                        filterSummary={data.filterSummary}
                        onClearFilters={onClearTableFilters}
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
