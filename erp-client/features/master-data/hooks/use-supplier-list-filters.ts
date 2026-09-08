"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import {
    csvFilterValue,
    parseLifecycleStatus,
    selectedSupplierOptionValues,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type { SupplierQualificationHealth } from "@/features/master-data/types"

/** 可被单独移除的已生效条件。 */
export type SupplierFilterKey =
    | "q"
    | "supplierQualificationHealth"
    | "supplierCapabilityCodes"
    | "supplierQualificationTypes"

/** 供应商列表：搜索 + 启停 + 资质状态 / 类型 + 供应能力。 */
export function useSupplierListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const {
        searchParams,
        patchUrl,
        q,
        pagination,
        setPagination,
        resetPagination,
        changePagination,
    } = useListUrl()
    const { searchDraft, setSearchDraft } = useSearchDraft(q, searchInputRef)

    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const supplierCapabilityCodesParam = searchParams.get(
        "supplierCapabilityCodes",
    )
    const supplierQualificationTypesParam = searchParams.get(
        "supplierQualificationTypes",
    )
    const supplierCapabilityCodes = React.useMemo(
        () =>
            selectedSupplierOptionValues(
                supplierCapabilityCodesParam,
                SUPPLIER_CAPABILITY_OPTIONS,
            ),
        [supplierCapabilityCodesParam],
    )
    const supplierQualificationTypes = React.useMemo(
        () =>
            selectedSupplierOptionValues(
                supplierQualificationTypesParam,
                SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
            ),
        [supplierQualificationTypesParam],
    )
    const supplierQualificationHealth =
        SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
            (option) =>
                option.value !== "all" &&
                option.value ===
                    searchParams.get("supplierQualificationHealth"),
        )?.value as SupplierQualificationHealth | undefined
    const metricKey = lifecycleStatus
    const hasStructuredSupplierFilters = Boolean(
        lifecycleStatus !== "all" ||
        supplierQualificationHealth ||
        supplierCapabilityCodes.length ||
        supplierQualificationTypes.length,
    )

    const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
        React.useState(hasStructuredSupplierFilters)
    const [supplierCapabilityCodesDraft, setSupplierCapabilityCodesDraft] =
        React.useState<string[]>(supplierCapabilityCodes)
    const [
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
    ] = React.useState<string[]>(supplierQualificationTypes)
    const [
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
    ] = React.useState<SupplierQualificationHealth | "all">(
        supplierQualificationHealth ?? "all",
    )

    const changeLifecycle = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            if (next === lifecycleStatus) return
            patchUrl({
                lifecycleStatus: next === "all" ? null : next,
                metricKey: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
        },
        [lifecycleStatus, patchUrl, resetPagination],
    )

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    const applySupplierFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            supplierCapabilityCodes: csvFilterValue(
                supplierCapabilityCodesDraft,
            ),
            supplierQualificationTypes: csvFilterValue(
                supplierQualificationTypesDraft,
            ),
            supplierQualificationHealth:
                supplierQualificationHealthDraft === "all"
                    ? null
                    : supplierQualificationHealthDraft,
            page: null,
        })
        resetPagination()
        setSupplierFilterPanelOpen(false)
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        supplierCapabilityCodesDraft,
        supplierQualificationHealthDraft,
        supplierQualificationTypesDraft,
    ])

    /** 移除单个普通筛选条件，保留当前启停 Tab。 */
    const removeFilter = React.useCallback(
        (key: SupplierFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "supplierQualificationHealth") {
                setSupplierQualificationHealthDraft("all")
            }
            if (key === "supplierCapabilityCodes") {
                setSupplierCapabilityCodesDraft([])
            }
            if (key === "supplierQualificationTypes") {
                setSupplierQualificationTypesDraft([])
            }
            patchUrl({ [key]: null, page: null })
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    /** 仅重置更多条件草稿；保留关键词、启停、资质状态及当前查询结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setSupplierCapabilityCodesDraft([])
        setSupplierQualificationTypesDraft([])
    }, [])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        supplierQualificationHealthDraft !==
            (supplierQualificationHealth ?? "all") ||
        [...supplierCapabilityCodesDraft].sort().join(",") !==
            [...supplierCapabilityCodes].sort().join(",") ||
        [...supplierQualificationTypesDraft].sort().join(",") !==
            [...supplierQualificationTypes].sort().join(",")

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSupplierCapabilityCodesDraft([])
        setSupplierQualificationTypesDraft([])
        setSupplierQualificationHealthDraft("all")
        setSupplierFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            supplierCapabilityCodes: null,
            supplierQualificationTypes: null,
            supplierQualificationHealth: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    React.useEffect(() => {
        setSupplierCapabilityCodesDraft(supplierCapabilityCodes)
        setSupplierQualificationTypesDraft(supplierQualificationTypes)
        setSupplierQualificationHealthDraft(
            supplierQualificationHealth ?? "all",
        )
    }, [
        lifecycleStatus,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
    ])

    return {
        q,
        lifecycleStatus,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        metricKey,
        hasStructuredSupplierFilters,
        searchDraft,
        setSearchDraft,
        supplierFilterPanelOpen,
        setSupplierFilterPanelOpen,
        supplierCapabilityCodesDraft,
        setSupplierCapabilityCodesDraft,
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
        hasPendingChanges,
        pagination,
        setPagination,
        changePagination,
        commitSearch,
        changeLifecycle,
        applySupplierFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
