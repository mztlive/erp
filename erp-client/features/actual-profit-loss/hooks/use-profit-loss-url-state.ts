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
import {
    COST_TYPE_CHIP_PREFIX,
    FULFILLMENT_MODE_CHIP_PREFIX,
} from "@/features/actual-profit-loss/hooks/profit-loss-filter-contract"
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
    const dimension = parseDimension(searchParams.get("dimension"))
    const customerId = searchParams.get("customerId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const qParam = searchParams.get("q") ?? ""
    const sort = searchParams.get("sort") ?? "actualProfitLossNet:asc"
    const page = parsePage(searchParams.get("page"))
    const pageSize = parsePageSize(searchParams.get("pageSize"))
    const benefitScenario =
        searchParams.get("benefitScenario")?.trim() || undefined
    const fulfillmentModesParam = searchParams.get("fulfillmentMode")
    const costTypesParam = searchParams.get("costType")
    const fulfillmentModes = React.useMemo(
        () => parseCsvValues(fulfillmentModesParam),
        [fulfillmentModesParam],
    )
    const costTypes = React.useMemo(
        () => parseCsvValues(costTypesParam),
        [costTypesParam],
    )
    const hasStructuredFilters = Boolean(
        benefitScenario || fulfillmentModes.length > 0 || costTypes.length > 0,
    )

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [benefitScenarioDraft, setBenefitScenarioDraft] = React.useState(
        benefitScenario ?? "",
    )
    const [fulfillmentModesDraft, setFulfillmentModesDraft] = React.useState<
        string[]
    >(() => [...fulfillmentModes])
    const [costTypesDraft, setCostTypesDraft] = React.useState<string[]>(() => [
        ...costTypes,
    ])
    const [filterPanelOpen, setFilterPanelOpen] =
        React.useState(hasStructuredFilters)
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: page - 1, pageSize }),
        [page, pageSize],
    )

    const patchUrl = React.useCallback(
        (patch: ProfitLossUrlPatch, options?: { replace?: boolean }) => {
            patchSearchParams({ router, pathname, searchParams }, patch, {
                replace: options?.replace ?? true,
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

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
              scopeId: "org-hq-finance",
              coverage,
              customerId,
              salesOrderId,
              benefitScenario,
              fulfillmentModes:
                  fulfillmentModes.length > 0 ? fulfillmentModes : undefined,
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
        setBenefitScenarioDraft(benefitScenario ?? "")
        setFulfillmentModesDraft([...fulfillmentModes])
        setCostTypesDraft([...costTypes])
    }, [benefitScenario, costTypes, fulfillmentModes])
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
            q: searchInput.trim() || null,
            benefitScenario: benefitScenarioDraft.trim() || null,
            fulfillmentMode: serializeCsvValues(fulfillmentModesDraft) || null,
            costType: serializeCsvValues(costTypesDraft) || null,
            page: null,
        })
        setFilterPanelOpen(false)
    }, [
        benefitScenarioDraft,
        costTypesDraft,
        fulfillmentModesDraft,
        patchUrl,
        searchInput,
    ])
    const clearAllFilters = React.useCallback(() => {
        setSearchInput("")
        setBenefitScenarioDraft("")
        setFulfillmentModesDraft([])
        setCostTypesDraft([])
        setFilterPanelOpen(false)
        patchUrl({
            q: null,
            coverage: null,
            customerId: null,
            salesOrderId: null,
            benefitScenario: null,
            fulfillmentMode: null,
            costType: null,
            page: null,
        })
    }, [patchUrl])
    const resetMoreFilters = React.useCallback(() => {
        setBenefitScenarioDraft("")
        setFulfillmentModesDraft([])
        setCostTypesDraft([])
        patchUrl({
            benefitScenario: null,
            fulfillmentMode: null,
            costType: null,
            page: null,
        })
    }, [patchUrl])
    const removeFilter = React.useCallback(
        (key: string) => {
            if (key === "q") {
                setSearchInput("")
                patchUrl({ q: null, page: null })
            } else if (key === "coverage") {
                patchUrl({ coverage: null, page: null })
            } else if (key === "customerId") {
                patchUrl({ customerId: null, page: null })
            } else if (key === "salesOrderId") {
                patchUrl({ salesOrderId: null, page: null })
            } else if (key === "benefitScenario") {
                setBenefitScenarioDraft("")
                patchUrl({ benefitScenario: null, page: null })
            } else if (key.startsWith(FULFILLMENT_MODE_CHIP_PREFIX)) {
                const value = key.slice(FULFILLMENT_MODE_CHIP_PREFIX.length)
                setFulfillmentModesDraft((current) =>
                    current.filter((item) => item !== value),
                )
                patchUrl({
                    fulfillmentMode:
                        serializeCsvValues(
                            fulfillmentModes.filter((item) => item !== value),
                        ) || null,
                    page: null,
                })
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
        [costTypes, fulfillmentModes, patchUrl],
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
    const handleCoverageChange = React.useCallback(
        (value: string) => {
            const next = value as ProfitLossCoverage
            patchUrl(
                next === "covered"
                    ? { coverage: null, page: null }
                    : { coverage: next, page: null },
            )
        },
        [patchUrl],
    )
    const handleDimensionChange = React.useCallback(
        (value: string) => patchUrl({ dimension: value, page: null }),
        [patchUrl],
    )

    return {
        query,
        periodPresetRaw: searchParams.get("periodPreset") ?? "",
        from,
        to,
        periodBasisUrl,
        periodBasisValid,
        coverage,
        dimension,
        customerId,
        salesOrderId,
        qParam,
        benefitScenario,
        fulfillmentModes,
        costTypes,
        analysisBlocked,
        analysisReady,
        searchInput,
        setSearchInput,
        searchInputRef,
        benefitScenarioDraft,
        setBenefitScenarioDraft,
        fulfillmentModesDraft,
        setFulfillmentModesDraft,
        costTypesDraft,
        setCostTypesDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        hasStructuredFilters,
        hasFilters,
        applyFilters,
        clearAllFilters,
        resetMoreFilters,
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
