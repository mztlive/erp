"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"

import { BusinessFailureState, PageScaffold } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { Button } from "@/components/ui/button"

import type { BusinessTag, CustomerQualityExportJob } from "../types"
import {
    useCustomerQualityQuery,
    useRefreshCustomerQualityMutation,
    useStartCustomerQualityExportMutation,
} from "../hooks/queries"
import { useCustomerQualityColumns } from "../hooks/use-customer-quality-columns"
import { useCustomerQualityNavigationState } from "../hooks/use-customer-quality-navigation-state"
import { useCustomerQualityPeriodState } from "../hooks/use-customer-quality-period-state"
import { useCustomerQualityRowFocus } from "../hooks/use-customer-quality-row-focus"
import { useCustomerQualitySearch } from "../hooks/use-customer-quality-search"
import {
    parseBusinessType,
    parseFundsReview,
    parseScenario,
} from "../lib/url-state"
import { CustomerQualityMainView } from "../components/customer-quality-main-view"
import { CustomerQualityPageSkeleton } from "../components/customer-quality-page-skeleton"
import { PeriodBlockerCard } from "../components/period-blocker-card"

export function CustomerQualityPage() {
    const searchParams = useSearchParams()

    const scenario = parseScenario(searchParams.get("scenario"))
    const fromParam = searchParams.get("from")
    const toParam = searchParams.get("to")
    const fundsReview = parseFundsReview(searchParams.get("fundsReview"))
    const businessType = parseBusinessType(searchParams.get("businessType"))
    const scaleTag = searchParams.get("scaleTag") ?? undefined
    const profitTag = searchParams.get("profitTag") ?? undefined
    const riskTag = searchParams.get("riskTag") ?? undefined
    const qParam = searchParams.get("q") ?? ""
    const chartDimension = searchParams.get("chartDimension") ?? undefined
    const chartCode = searchParams.get("chartCode") ?? undefined
    const customerId = searchParams.get("customerId") ?? undefined
    const focusCustomerId = searchParams.get("focusCustomerId") ?? undefined
    const focusMetric = searchParams.get("focusMetric") ?? undefined
    const periodPreset = searchParams.get("periodPreset") ?? undefined
    // 数据范围固定为当前角色默认范围；不接受 URL 隐形覆盖（URL 参数与控件一一对应）
    const scopeId = "scope:team:sales-east"

    const nav = useCustomerQualityNavigationState()
    const search = useCustomerQualitySearch({
        qParam,
        patchUrl: nav.patchUrl,
    })
    const period = useCustomerQualityPeriodState({
        scenario,
        fromParam,
        toParam,
        periodPreset,
        fundsReview,
        businessType,
        scaleTag,
        profitTag,
        riskTag,
        qParam,
        sort: nav.sort,
        chartDimension,
        chartCode,
        customerId,
        scopeId,
        pagination: nav.pagination,
        patchUrl: nav.patchUrl,
    })

    const viewQuery = useCustomerQualityQuery(period.analysisQuery)
    const exportMutation = useStartCustomerQualityExportMutation()
    const refreshMutation = useRefreshCustomerQualityMutation()

    const [tagDialog, setTagDialog] = React.useState<BusinessTag | null>(null)
    const [exportJob, setExportJob] =
        React.useState<CustomerQualityExportJob | null>(null)
    const [refreshError, setRefreshError] = React.useState<string | null>(null)

    async function handleRefresh() {
        setRefreshError(null)
        try {
            await refreshMutation.mutateAsync()
            await viewQuery.refetch()
        } catch (error) {
            setRefreshError(
                getErrorMessage(error, "本次刷新未成功，已保留上次成功结果。"),
            )
        }
    }

    const data = viewQuery.data

    // 定位失败降级：目标客户不在当前页/排序结果时滚动到明细表顶部
    const tableSectionRef = React.useRef<HTMLDivElement>(null)
    const scrollToTableTop = React.useCallback(() => {
        tableSectionRef.current?.scrollIntoView({
            behavior: "smooth",
            block: "start",
        })
        tableSectionRef.current?.focus({ preventScroll: true })
    }, [])

    useCustomerQualityRowFocus({
        focusCustomerId,
        focusMetric,
        data,
        scrollToTableTop,
    })

    const columns = useCustomerQualityColumns({
        data,
        returnTo: nav.returnTo,
        businessType,
        onTagClick: setTagDialog,
    })

    const scaleDimension = data?.dimensions.find((d) => d.key === "scale")
    const profitDimension = data?.dimensions.find((d) => d.key === "profit")
    const natureDimension = data?.dimensions.find(
        (d) => d.key === "businessNature",
    )

    const chartFilterSummary = React.useMemo(() => {
        if (!chartDimension || !chartCode || !data) return null
        const dim = data.dimensions.find((d) => d.key === chartDimension)
        const item = dim?.items.find((i) => i.code === chartCode)
        if (!dim || !item) return null
        return {
            dimensionTitle: dim.title,
            itemLabel: item.label,
            resultCount: data.customers.filteredTotal,
        }
    }, [chartDimension, chartCode, data])

    async function handleExport() {
        if (!data || !period.analysisQuery) return
        const job = await exportMutation.mutateAsync({
            query: period.analysisQuery,
            filterSummary: data.filterSummary,
            projectionWatermark: data.freshness.sourceWatermark,
            permissionVersion: data.scope.permissionVersion,
            rowCount: data.customers.filteredTotal,
        })
        setExportJob(job)
    }

    function clearAllFilters() {
        search.setSearchInput("")
        // P4：清除筛选保留排序与期间（分析页维度参数）
        nav.patchUrl({
            q: null,
            scaleTag: null,
            profitTag: null,
            riskTag: null,
            chartDimension: null,
            chartCode: null,
            businessType: null,
            fundsReview: null,
            customerId: null,
            focusMetric: null,
        })
        nav.resetPage()
    }

    function clearTableFilters() {
        // P4：清除筛选保留排序与期间（分析页维度参数）
        nav.patchUrl({
            q: null,
            scaleTag: null,
            profitTag: null,
            riskTag: null,
            chartDimension: null,
            chartCode: null,
            businessType: null,
            fundsReview: null,
            customerId: null,
            focusMetric: null,
        })
        nav.resetPage()
    }

    // —— Loading shells ——
    if (
        period.periodPolicyQuery.isPending ||
        (!period.periodWriteDone && !period.needsPeriodBlocker)
    ) {
        return <CustomerQualityPageSkeleton variant="policy-loading" />
    }

    if (period.periodPolicyQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="期间配置加载失败"
                    error={period.periodPolicyQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() =>
                                void period.periodPolicyQuery.refetch()
                            }
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    // —— Period blocker ——
    if (period.needsPeriodBlocker) {
        return (
            <PageScaffold>
                <PeriodBlockerCard
                    periodPolicy={period.periodPolicy}
                    explicitFrom={period.explicitFrom}
                    explicitTo={period.explicitTo}
                    onFromChange={period.setExplicitFrom}
                    onToChange={period.setExplicitTo}
                    onApplyExplicit={period.applyExplicitPeriod}
                    onApplyPreset={period.applyPreset}
                />
            </PageScaffold>
        )
    }

    if (viewQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="经营质量数据加载失败"
                    error={viewQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void viewQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (viewQuery.isPending || !data) {
        return <CustomerQualityPageSkeleton variant="view-loading" />
    }

    if (data.emptyKind === "forbidden") {
        return (
            <PageScaffold>
                <BusinessFailureState
                    kind="permission"
                    title="无客户经营质量权限"
                    description="当前账号缺少经营质量模块权限。敏感明细已不展示。"
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href="/workspace" />}
                        >
                            返回工作台
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    // customerId 深链参数的显性名称（在当前页数据中反查客户名）
    const chipCustomerName = data.customers.items.find(
        (c) => c.customerId === customerId,
    )?.customerName

    const showClearFilters = Boolean(
        qParam ||
            scaleTag ||
            profitTag ||
            riskTag ||
            chartCode ||
            businessType ||
            fundsReview === "reviewed_only" ||
            customerId,
    )

    return (
        <CustomerQualityMainView
            data={data}
            refreshError={refreshError}
            refreshing={viewQuery.isFetching && !viewQuery.isPending}
            onRefresh={() => {
                void handleRefresh()
            }}
            exportPending={exportMutation.isPending}
            onExport={() => {
                void handleExport()
            }}
            exportJob={exportJob}
            resolvedFrom={period.resolvedFrom}
            resolvedTo={period.resolvedTo}
            periodInvalid={period.periodInvalid}
            presets={period.periodPolicy?.presets}
            periodPreset={periodPreset}
            fundsReview={fundsReview}
            businessType={businessType}
            sort={nav.sort}
            searchInput={search.searchInput}
            searchInputRef={search.searchInputRef}
            onSearchInputChange={search.setSearchInput}
            customerId={customerId}
            chipCustomerName={chipCustomerName}
            showClearFilters={showClearFilters}
            onClearFilters={clearAllFilters}
            onPresetSelect={(id) => {
                const preset = period.periodPolicy?.presets?.find(
                    (p) => p.id === id,
                )
                if (preset) {
                    period.applyPreset(preset.id, preset.from, preset.to)
                    nav.resetPage()
                } else nav.patchUrl({ periodPreset: null })
            }}
            patchUrl={nav.patchUrl}
            resetPage={nav.resetPage}
            scaleDimension={scaleDimension}
            profitDimension={profitDimension}
            natureDimension={natureDimension}
            chartDimension={chartDimension}
            chartCode={chartCode}
            chartFilterSummary={chartFilterSummary}
            setPagination={nav.setPagination}
            columns={columns}
            pagination={nav.pagination}
            onPaginationChange={nav.handlePaginationChange}
            tableSorting={nav.tableSorting}
            onSortingChange={nav.handleTableSortingChange}
            tableSectionRef={tableSectionRef}
            onFocusTable={scrollToTableTop}
            onClearTableFilters={clearTableFilters}
            tagDialog={tagDialog}
            onTagDialogOpenChange={(open) => {
                if (!open) setTagDialog(null)
            }}
        />
    )
}
