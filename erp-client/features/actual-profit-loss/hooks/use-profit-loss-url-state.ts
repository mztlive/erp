"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    parseCoverage,
    parseCsvValues,
    parseDimension,
    parsePage,
    parsePageSize,
    parsePreset,
    resolvePeriod,
    serializeCsvValues,
} from "@/features/actual-profit-loss/lib/url-state"
import type {
    ProfitLossCoverage,
    ProfitLossPeriodBasisConfig,
    ProfitLossQuery,
} from "@/features/actual-profit-loss/types"
import { COST_TYPE_CHIP_PREFIX } from "@/features/actual-profit-loss/hooks/profit-loss-filter-contract"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

export type ProfitLossUrlPatch = Record<string, string | null | undefined>

type Options = {
    basisConfig: ProfitLossPeriodBasisConfig | undefined
    basisResolved: boolean
}

/** 实际盈亏 Applied/Draft/UI 筛选、分页、排序和查询契约。 */
export function useProfitLossUrlState({ basisConfig, basisResolved }: Options) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const periodPreset = parsePreset(searchParams.get("periodPreset"))
    const resolvedDefault = resolvePeriod(periodPreset)
    const from = searchParams.get("from") ?? resolvedDefault.from
    const to = searchParams.get("to") ?? resolvedDefault.to
    const periodBasisUrl = searchParams.get("periodBasis") ?? ""
    const coverage = parseCoverage(searchParams.get("coverage"))
    const [coverageDraft, setCoverageDraft] = React.useState(coverage)
    const dimension = parseDimension(searchParams.get("dimension"))
    const customerId = searchParams.get("customerId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const qParam = searchParams.get("q") ?? ""
    const sort = searchParams.get("sort") ?? "actualProfitLossNet:asc"
    const page = parsePage(searchParams.get("page"))
    const pageSize = parsePageSize(searchParams.get("pageSize"))
    const benefitScenario =
        searchParams.get("benefitScenario")?.trim() || undefined
    const attributionUsersParam = searchParams.get("attributionUserIds")
    const attributionOrgsParam = searchParams.get("attributionOrgUnitIds")
    const attributionGroup = searchParams.get("attributionGroup") ?? undefined
    const attributionUserIds = React.useMemo(
        () => parseCsvValues(attributionUsersParam),
        [attributionUsersParam],
    )
    const attributionOrgUnitIds = React.useMemo(
        () => parseCsvValues(attributionOrgsParam),
        [attributionOrgsParam],
    )
    const [attributionUsersDraft, setAttributionUsersDraft] =
        React.useState<string[]>(attributionUserIds)
    const [attributionOrgsDraft, setAttributionOrgsDraft] = React.useState<
        string[]
    >(attributionOrgUnitIds)
    React.useEffect(() => {
        setAttributionUsersDraft(attributionUserIds)
        setAttributionOrgsDraft(attributionOrgUnitIds)
    }, [attributionUserIds, attributionOrgUnitIds])
    const costTypesParam = searchParams.get("costType")
    const costTypes = React.useMemo(
        () => parseCsvValues(costTypesParam),
        [costTypesParam],
    )
    const hasStructuredFilters = Boolean(
        attributionGroup ||
        benefitScenario ||
        costTypes.length > 0 ||
        attributionUserIds.length > 0 ||
        attributionOrgUnitIds.length > 0,
    )

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [benefitScenarioDraft, setBenefitScenarioDraft] = React.useState(
        benefitScenario ?? "",
    )
    const [costTypesDraft, setCostTypesDraft] = React.useState<string[]>(() => [
        ...costTypes,
    ])
    // 深链只显示已生效标签，不自动打开浮层。
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: page - 1, pageSize }),
        [page, pageSize],
    )

    const patchUrl = React.useCallback(
        (patch: ProfitLossUrlPatch, options?: { replace?: boolean }) => {
            const scopedPatch =
                patch.page === null ? { ...patch, scopeVersion: null } : patch
            patchSearchParams({ router, pathname, searchParams }, scopedPatch, {
                replace: options?.replace ?? true,
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    React.useEffect(() => {
        if (page > 1 && !searchParams.get("scopeVersion"))
            patchUrl({ page: null })
    }, [page, searchParams, patchUrl])

    React.useEffect(() => {
        if (!basisConfig?.configuredPeriodBasis || periodBasisUrl) {
            return
        }
        patchUrl({
            periodBasis: basisConfig.configuredPeriodBasis,
            from,
            to,
        })
    }, [
        basisConfig?.configuredPeriodBasis,
        from,
        patchUrl,
        periodBasisUrl,
        searchParams,
        to,
    ])

    React.useEffect(() => {
        const retiredDimension = searchParams.get("dimension")
        if (
            searchParams.has("fulfillmentMode") ||
            retiredDimension === "fulfillment" ||
            retiredDimension === "cost_type"
        ) {
            patchUrl({
                fulfillmentMode: null,
                dimension:
                    retiredDimension === "fulfillment" ||
                    retiredDimension === "cost_type"
                        ? null
                        : retiredDimension,
                page: null,
            })
        }
    }, [searchParams, patchUrl])

    const allowedCodes = React.useMemo(
        () =>
            new Set(
                basisConfig?.allowedPeriodBases.map((basis) => basis.code) ??
                    [],
            ),
        [basisConfig?.allowedPeriodBases],
    )
    const periodBasisValid =
        Boolean(periodBasisUrl) && allowedCodes.has(periodBasisUrl)
    const analysisBlocked =
        basisResolved &&
        !basisConfig?.configuredPeriodBasis &&
        !periodBasisValid
    const analysisReady = periodBasisValid && !analysisBlocked
    const query: ProfitLossQuery | null = analysisReady
        ? {
              from,
              to,
              periodBasis: periodBasisUrl,
              scopeId: "authorized",
              scopeVersion: searchParams.get("scopeVersion") ?? undefined,
              coverage,
              customerId,
              salesOrderId,
              attributionUserIds,
              attributionOrgUnitIds,
              attributionGroup,
              benefitScenario,
              costTypes: costTypes.length > 0 ? costTypes : undefined,
              dimension,
              q: qParam || undefined,
              sort,
              page,
              pageSize,
          }
        : null

    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])
    React.useEffect(() => {
        setCoverageDraft(coverage)
    }, [coverage])
    React.useEffect(() => {
        setBenefitScenarioDraft(benefitScenario ?? "")
        setCostTypesDraft([...costTypes])
    }, [benefitScenario, costTypes])
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            if (
                target?.tagName === "INPUT" ||
                target?.tagName === "TEXTAREA" ||
                target?.tagName === "SELECT" ||
                target?.isContentEditable ||
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"], [data-slot="popover-content"], [data-slot="combobox-content"]',
                )
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    const tableSorting = React.useMemo<SortingState>(() => {
        const [id, direction] = sort.split(":")
        return [{ id, desc: direction === "desc" }]
    }, [sort])
    const handleTableSortingChange = React.useCallback(
        (next: SortingState) => {
            const nextSort = next[0]
            patchUrl({
                sort: nextSort
                    ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                    : "actualProfitLossNet:asc",
                page: null,
            })
        },
        [patchUrl],
    )

    const hasFilters = Boolean(
        qParam.trim() ||
        customerId ||
        salesOrderId ||
        coverage !== "covered" ||
        hasStructuredFilters,
    )
    const applyFilters = React.useCallback(() => {
        patchUrl({
            attributionUserIds:
                serializeCsvValues(attributionUsersDraft) || null,
            attributionOrgUnitIds:
                serializeCsvValues(attributionOrgsDraft) || null,
            q: searchInput.trim() || null,
            coverage: coverageDraft === "covered" ? null : coverageDraft,
            benefitScenario: benefitScenarioDraft.trim() || null,
            costType: serializeCsvValues(costTypesDraft) || null,
            page: null,
        })
        setFilterPanelOpen(false)
    }, [
        attributionUsersDraft,
        attributionOrgsDraft,
        benefitScenarioDraft,
        costTypesDraft,
        coverageDraft,
        patchUrl,
        searchInput,
    ])
    const clearAllFilters = React.useCallback(() => {
        setSearchInput("")
        setCoverageDraft("covered")
        setBenefitScenarioDraft("")
        setCostTypesDraft([])
        setAttributionUsersDraft([])
        setAttributionOrgsDraft([])
        setFilterPanelOpen(false)
        patchUrl({
            q: null,
            coverage: null,
            customerId: null,
            attributionUserIds: null,
            attributionOrgUnitIds: null,
            attributionGroup: null,
            salesOrderId: null,
            benefitScenario: null,
            fulfillmentMode: null,
            costType: null,
            page: null,
        })
    }, [patchUrl])
    const resetMoreFilters = React.useCallback(() => {
        setBenefitScenarioDraft("")
        setCostTypesDraft([])
        setAttributionUsersDraft([])
        setAttributionOrgsDraft([])
    }, [])

    /** 取消、关闭、Esc 和外点只恢复低频草稿，保留搜索词。 */
    const cancelMoreFilters = React.useCallback(() => {
        setAttributionUsersDraft([...attributionUserIds])
        setAttributionOrgsDraft([...attributionOrgUnitIds])
        setBenefitScenarioDraft(benefitScenario ?? "")
        setCostTypesDraft([...costTypes])
        setFilterPanelOpen(false)
    }, [attributionOrgUnitIds, attributionUserIds, benefitScenario, costTypes])

    const hasPendingChanges =
        serializeCsvValues(attributionUsersDraft) !==
            serializeCsvValues(attributionUserIds) ||
        serializeCsvValues(attributionOrgsDraft) !==
            serializeCsvValues(attributionOrgUnitIds) ||
        searchInput.trim() !== qParam.trim() ||
        coverageDraft !== coverage ||
        benefitScenarioDraft.trim() !== (benefitScenario ?? "") ||
        serializeCsvValues(costTypesDraft) !== serializeCsvValues(costTypes)
    const removeFilter = React.useCallback(
        (key: string) => {
            if (key === "attributionGroup") {
                patchUrl({ attributionGroup: null, page: null })
            } else if (key === "attributionUserIds") {
                setAttributionUsersDraft([])
                patchUrl({ attributionUserIds: null, page: null })
            } else if (key === "attributionOrgUnitIds") {
                setAttributionOrgsDraft([])
                patchUrl({ attributionOrgUnitIds: null, page: null })
            } else if (key === "q") {
                setSearchInput("")
                patchUrl({ q: null, page: null })
            } else if (key === "coverage") {
                setCoverageDraft("covered")
                patchUrl({ coverage: null, page: null })
            } else if (key === "customerId") {
                patchUrl({ customerId: null, page: null })
            } else if (key === "salesOrderId") {
                patchUrl({ salesOrderId: null, page: null })
            } else if (key === "benefitScenario") {
                setBenefitScenarioDraft("")
                patchUrl({ benefitScenario: null, page: null })
            } else if (key.startsWith(COST_TYPE_CHIP_PREFIX)) {
                const value = key.slice(COST_TYPE_CHIP_PREFIX.length)
                setCostTypesDraft((current) =>
                    current.filter((item) => item !== value),
                )
                patchUrl({
                    costType:
                        serializeCsvValues(
                            costTypes.filter((item) => item !== value),
                        ) || null,
                    page: null,
                })
            }
        },
        [costTypes, patchUrl],
    )
    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            patchUrl({
                page: next.pageIndex > 0 ? String(next.pageIndex + 1) : null,
                pageSize: next.pageSize === 20 ? null : String(next.pageSize),
            })
        },
        [patchUrl],
    )
    const handleCoverageChange = React.useCallback((value: string) => {
        setCoverageDraft(value as ProfitLossCoverage)
    }, [])
    const handleDimensionChange = React.useCallback(
        (value: string) => patchUrl({ dimension: value, page: null }),
        [patchUrl],
    )

    return {
        query,
        attributionUserIds,
        attributionOrgUnitIds,
        attributionGroup,
        attributionUsersDraft,
        setAttributionUsersDraft,
        attributionOrgsDraft,
        setAttributionOrgsDraft,
        periodPresetRaw: searchParams.get("periodPreset") ?? "",
        from,
        to,
        periodBasisUrl,
        periodBasisValid,
        coverage,
        coverageDraft,
        dimension,
        customerId,
        salesOrderId,
        qParam,
        benefitScenario,
        costTypes,
        analysisBlocked,
        analysisReady,
        searchInput,
        setSearchInput,
        searchInputRef,
        benefitScenarioDraft,
        setBenefitScenarioDraft,
        costTypesDraft,
        setCostTypesDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        hasStructuredFilters,
        hasFilters,
        applyFilters,
        clearAllFilters,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
        removeFilter,
        pagination,
        setPagination: handlePaginationChange,
        tableSorting,
        handleTableSortingChange,
        patchUrl,
        handleCoverageChange,
        handleDimensionChange,
    }
}
