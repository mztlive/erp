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
    | "owner_user_ids"
    | "capability_owner_user_ids"
    | "org_unit_ids"

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
    const ownerUserIds = searchParams.get("owner_user_ids") ?? ""
    const capabilityOwnerUserIds =
        searchParams.get("capability_owner_user_ids") ?? ""
    const orgUnitIds = searchParams.get("org_unit_ids") ?? ""
    const includeDescendants =
        searchParams.get("include_descendants") === "true"
    const hasStructuredSupplierFilters = Boolean(
        lifecycleStatus !== "all" ||
        supplierQualificationHealth ||
        supplierCapabilityCodes.length ||
        supplierQualificationTypes.length ||
        ownerUserIds ||
        capabilityOwnerUserIds ||
        orgUnitIds,
    )

    // 深链只显示已生效标签，不自动打开筛选浮层。
    const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
        React.useState(false)
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
    const [ownerUserIdsDraft, setOwnerUserIdsDraft] =
        React.useState(ownerUserIds)
    const [capabilityOwnerUserIdsDraft, setCapabilityOwnerUserIdsDraft] =
        React.useState(capabilityOwnerUserIds)
    const [orgUnitIdsDraft, setOrgUnitIdsDraft] = React.useState(orgUnitIds)
    const [includeDescendantsDraft, setIncludeDescendantsDraft] =
        React.useState(includeDescendants)

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
            owner_user_ids: ownerUserIdsDraft || null,
            capability_owner_user_ids: capabilityOwnerUserIdsDraft || null,
            org_unit_ids: orgUnitIdsDraft.trim() || null,
            include_descendants:
                orgUnitIdsDraft.trim() && includeDescendantsDraft
                    ? "true"
                    : null,
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
        ownerUserIdsDraft,
        capabilityOwnerUserIdsDraft,
        orgUnitIdsDraft,
        includeDescendantsDraft,
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
            if (key === "owner_user_ids") setOwnerUserIdsDraft("")
            if (key === "capability_owner_user_ids")
                setCapabilityOwnerUserIdsDraft("")
            if (key === "org_unit_ids") {
                setOrgUnitIdsDraft("")
                setIncludeDescendantsDraft(false)
                patchUrl({
                    org_unit_ids: null,
                    include_descendants: null,
                    page: null,
                })
                resetPagination()
                return
            }
            patchUrl({ [key]: null, page: null })
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    /** 仅重置低频草稿；保留关键词、资质状态、供应能力及当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setSupplierQualificationTypesDraft([])
        setOwnerUserIdsDraft("")
        setCapabilityOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
    }, [])

    /** 取消、外点和 Esc 只恢复低频草稿，保留搜索、资质状态和供应能力。 */
    const cancelMoreFilters = React.useCallback(() => {
        setSupplierQualificationTypesDraft(supplierQualificationTypes)
        setOwnerUserIdsDraft(ownerUserIds)
        setCapabilityOwnerUserIdsDraft(capabilityOwnerUserIds)
        setOrgUnitIdsDraft(orgUnitIds)
        setIncludeDescendantsDraft(includeDescendants)
        setSupplierFilterPanelOpen(false)
    }, [
        capabilityOwnerUserIds,
        includeDescendants,
        orgUnitIds,
        ownerUserIds,
        supplierQualificationTypes,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        supplierQualificationHealthDraft !==
            (supplierQualificationHealth ?? "all") ||
        [...supplierCapabilityCodesDraft].sort().join(",") !==
            [...supplierCapabilityCodes].sort().join(",") ||
        [...supplierQualificationTypesDraft].sort().join(",") !==
            [...supplierQualificationTypes].sort().join(",") ||
        ownerUserIdsDraft !== ownerUserIds ||
        capabilityOwnerUserIdsDraft !== capabilityOwnerUserIds ||
        orgUnitIdsDraft.trim() !== orgUnitIds.trim() ||
        includeDescendantsDraft !== includeDescendants

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSupplierCapabilityCodesDraft([])
        setSupplierQualificationTypesDraft([])
        setSupplierQualificationHealthDraft("all")
        setOwnerUserIdsDraft("")
        setCapabilityOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
        setSupplierFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            supplierCapabilityCodes: null,
            supplierQualificationTypes: null,
            supplierQualificationHealth: null,
            owner_user_ids: null,
            capability_owner_user_ids: null,
            org_unit_ids: null,
            include_descendants: null,
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
        setOwnerUserIdsDraft(ownerUserIds)
        setCapabilityOwnerUserIdsDraft(capabilityOwnerUserIds)
        setOrgUnitIdsDraft(orgUnitIds)
        setIncludeDescendantsDraft(includeDescendants)
    }, [
        lifecycleStatus,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
        ownerUserIds,
        capabilityOwnerUserIds,
        orgUnitIds,
        includeDescendants,
    ])

    return {
        q,
        lifecycleStatus,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        ownerUserIds,
        capabilityOwnerUserIds,
        orgUnitIds,
        includeDescendants,
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
        ownerUserIdsDraft,
        setOwnerUserIdsDraft,
        capabilityOwnerUserIdsDraft,
        setCapabilityOwnerUserIdsDraft,
        orgUnitIdsDraft,
        setOrgUnitIdsDraft,
        includeDescendantsDraft,
        setIncludeDescendantsDraft,
        hasPendingChanges,
        pagination,
        setPagination,
        changePagination,
        commitSearch,
        changeLifecycle,
        applySupplierFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
    }
}
