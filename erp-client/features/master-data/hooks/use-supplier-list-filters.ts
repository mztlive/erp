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
    const supplierQualificationHealth = SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
        (option) =>
            option.value !== "all" &&
            option.value === searchParams.get("supplierQualificationHealth"),
    )?.value as SupplierQualificationHealth | undefined
    const metricKey = searchParams.get("metricKey") ?? "all"
    const hasStructuredSupplierFilters = Boolean(
        lifecycleStatus !== "all" ||
        supplierQualificationHealth ||
        supplierCapabilityCodes.length ||
        supplierQualificationTypes.length,
    )

    const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
        React.useState(hasStructuredSupplierFilters)
    const [lifecycleStatusDraft, setLifecycleStatusDraft] =
        React.useState(lifecycleStatus)
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

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    const applySupplierFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
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
    }, [
        lifecycleStatusDraft,
        patchUrl,
        resetPagination,
        searchDraft,
        supplierCapabilityCodesDraft,
        supplierQualificationHealthDraft,
        supplierQualificationTypesDraft,
    ])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setLifecycleStatusDraft("all")
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
        setLifecycleStatusDraft(lifecycleStatus)
        setSupplierCapabilityCodesDraft(supplierCapabilityCodes)
        setSupplierQualificationTypesDraft(supplierQualificationTypes)
        setSupplierQualificationHealthDraft(
            supplierQualificationHealth ?? "all",
        )
        setSupplierFilterPanelOpen(hasStructuredSupplierFilters)
    }, [
        hasStructuredSupplierFilters,
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
        lifecycleStatusDraft,
        setLifecycleStatusDraft,
        supplierCapabilityCodesDraft,
        setSupplierCapabilityCodesDraft,
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
        pagination,
        setPagination,
        changePagination,
        commitSearch,
        applySupplierFilters,
        clearAllFilters,
    }
}
