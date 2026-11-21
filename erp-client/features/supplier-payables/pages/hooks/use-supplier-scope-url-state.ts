"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSearchParams } from "@/lib/navigation"

import { countScopeIds, parseScopeIdList } from "@/lib/funds-scope"
import { useScopeUrlPagination } from "@/lib/use-scope-url-pagination"
import { useSearchDraft } from "@/lib/use-search-draft"
import type { SupplierScopeQuery } from "@/features/supplier-payables/api/scoped"
import { parseView } from "@/features/supplier-payables/lib/url-state"

export type SupplierScopeFilterKey =
    | "q"
    | "procurementOwnerUserIds"
    | "operatorUserIds"
    | "orgUnitIds"
    | "supplierId"
    | "purchaseOrderId"
    | "payableAccountId"

export type SupplierScopeChip = Readonly<{
    key: SupplierScopeFilterKey
    label: string
}>

/** 供应商往来范围查询 URL 状态：有效条件进入 URL 与 Query key。 */
export function useSupplierScopeUrlState() {
    const pageSize = 20
    const searchParams = useSearchParams()
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canUseOwner = hasPermission(permissions, "procurement_person:list")
    const canUseBusiness = hasPermission(permissions, "business_person:list")
    const canUseOrg = hasPermission(permissions, "org_unit:list")
    const unavailableFilters = React.useMemo(
        () =>
            Object.fromEntries(
                [
                    [canUseOwner, "procurementOwnerUserIds"] as const,
                    [canUseBusiness, "operatorUserIds"] as const,
                    [canUseOrg, "orgUnitIds"] as const,
                    [canUseOrg, "includeDescendants"] as const,
                ]
                    .filter(([allowed]) => !allowed)
                    .map(([, key]) => [key, null]),
            ),
        [canUseOwner, canUseBusiness, canUseOrg],
    )
    const hasUnavailableFilters = Object.keys(unavailableFilters).some((key) =>
        searchParams.has(key),
    )

    const viewParam = parseView(searchParams.get("view"))
    const view: SupplierScopeQuery["view"] =
        viewParam === "unallocated" ? "payable" : viewParam
    const operatorLabel =
        view === "payment"
            ? "付款经办人"
            : view === "purchase_invoice"
              ? "收票经办人"
              : "经办人"
    const qParam = searchParams.get("q") ?? ""
    const procurementOwnerUserIds = canUseOwner
        ? parseScopeIdList(searchParams.get("procurementOwnerUserIds"))
        : undefined
    const operatorUserIds = canUseBusiness
        ? parseScopeIdList(searchParams.get("operatorUserIds"))
        : undefined
    const orgUnitIds = canUseOrg
        ? parseScopeIdList(searchParams.get("orgUnitIds"))
        : undefined
    const includeDescendants =
        canUseOrg && searchParams.get("includeDescendants") === "true"
    const supplierId = searchParams.get("supplierId") ?? undefined
    const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
    const payableAccountId = searchParams.get("payableAccountId") ?? undefined
    const scopeVersion = hasUnavailableFilters
        ? undefined
        : (searchParams.get("scopeVersion") ?? undefined)
    const {
        pageFromUrl: rawPage,
        pagination: rawPagination,
        patchUrl: patchPaginationUrl,
        handlePaginationChange,
    } = useScopeUrlPagination({ view, pageSize, scopeVersion })

    const pageFromUrl = hasUnavailableFilters ? 1 : rawPage
    const pagination = hasUnavailableFilters
        ? { ...rawPagination, pageIndex: 0 }
        : rawPagination
    const patchUrl = React.useCallback(
        (
            patch: Parameters<typeof patchPaginationUrl>[0],
            options?: { replace?: boolean },
        ) =>
            patchPaginationUrl(
                account.data ? { ...patch, ...unavailableFilters } : patch,
                options,
            ),
        [account.data, patchPaginationUrl, unavailableFilters],
    )
    React.useEffect(() => {
        if (account.data && hasUnavailableFilters) {
            patchUrl({ scopeVersion: null, page: null }, { replace: true })
        }
    }, [account.data, hasUnavailableFilters, patchUrl])

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const { searchDraft, setSearchDraft } = useSearchDraft(qParam, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })
    const [procurementOwnerDraftValue, setProcurementOwnerDraft] =
        React.useState(procurementOwnerUserIds ?? "")
    const [operatorDraftValue, setOperatorDraft] = React.useState(
        operatorUserIds ?? "",
    )
    const [orgDraftValue, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraftValue, setDescendantsDraft] =
        React.useState(includeDescendants)
    const [panelOpen, setPanelOpen] = React.useState(false)

    const procurementOwnerDraft = canUseOwner ? procurementOwnerDraftValue : ""
    const operatorDraft = canUseBusiness ? operatorDraftValue : ""
    const orgDraft = canUseOrg ? orgDraftValue : ""
    const descendantsDraft = canUseOrg ? descendantsDraftValue : false
    React.useEffect(() => {
        if (!canUseOwner) setProcurementOwnerDraft("")
        if (!canUseBusiness) setOperatorDraft("")
        if (!canUseOrg) setOrgDraft("")
        if (!canUseOrg) setDescendantsDraft(false)
    }, [canUseOwner, canUseBusiness, canUseOrg])

    const query: SupplierScopeQuery = React.useMemo(
        () => ({
            view,
            page: pageFromUrl,
            pageSize,
            scopeVersion,
            procurementOwnerUserIds,
            operatorUserIds,
            orgUnitIds,
            includeDescendants:
                orgUnitIds && includeDescendants ? true : undefined,
            q: qParam || undefined,
            supplierId,
            purchaseOrderId,
            payableAccountId,
        }),
        [
            includeDescendants,
            operatorUserIds,
            orgUnitIds,
            pageFromUrl,
            payableAccountId,
            procurementOwnerUserIds,
            purchaseOrderId,
            qParam,
            scopeVersion,
            supplierId,
            view,
        ],
    )

    const hasActiveFilters = Boolean(
        qParam.trim() ||
        procurementOwnerUserIds ||
        operatorUserIds ||
        orgUnitIds ||
        supplierId ||
        purchaseOrderId ||
        payableAccountId,
    )

    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                procurementOwnerUserIds: procurementOwnerDraft.trim() || null,
                operatorUserIds: operatorDraft.trim() || null,
                orgUnitIds: orgDraft.trim() || null,
                includeDescendants:
                    orgDraft.trim() && descendantsDraft ? "true" : null,
                scopeVersion: null,
                page: null,
            },
            { replace: true },
        )
        setPanelOpen(false)
    }, [
        descendantsDraft,
        operatorDraft,
        orgDraft,
        patchUrl,
        procurementOwnerDraft,
        searchDraft,
    ])

    const removeFilter = React.useCallback(
        (key: SupplierScopeFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "procurementOwnerUserIds") setProcurementOwnerDraft("")
            if (key === "operatorUserIds") setOperatorDraft("")
            if (key === "orgUnitIds") {
                setOrgDraft("")
                setDescendantsDraft(false)
            }
            const patch: Record<string, string | null | undefined> = {
                [key]: null,
                scopeVersion: null,
                page: null,
            }
            if (key === "orgUnitIds") patch.includeDescendants = null
            patchUrl(patch, { replace: true })
        },
        [patchUrl, setSearchDraft],
    )

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setProcurementOwnerDraft("")
        setOperatorDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                procurementOwnerUserIds: null,
                operatorUserIds: null,
                orgUnitIds: null,
                includeDescendants: null,
                supplierId: null,
                purchaseOrderId: null,
                payableAccountId: null,
                scopeVersion: null,
                page: null,
            },
            { replace: true },
        )
    }, [patchUrl, setSearchDraft])

    /** 只清经办人和业务组织草稿；保留搜索与采购负责人。 */
    const resetMoreFilters = React.useCallback(() => {
        setOperatorDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
    }, [])

    /** 取消、关闭、Esc 和外点只恢复低频草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setOperatorDraft(operatorUserIds ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [includeDescendants, operatorUserIds, orgUnitIds])

    const changeView = React.useCallback(
        (nextView: SupplierScopeQuery["view"]) => {
            patchUrl(
                { view: nextView, scopeVersion: null, page: null },
                { replace: true },
            )
        },
        [patchUrl],
    )

    const appliedChips = React.useMemo<readonly SupplierScopeChip[]>(() => {
        const chips: SupplierScopeChip[] = []
        const queryText = qParam.trim()
        if (queryText) chips.push({ key: "q", label: `搜索：${queryText}` })
        if (procurementOwnerUserIds) {
            chips.push({
                key: "procurementOwnerUserIds",
                label: `采购负责人：已选 ${countScopeIds(procurementOwnerUserIds)} 人`,
            })
        }
        if (operatorUserIds) {
            chips.push({
                key: "operatorUserIds",
                label: `${operatorLabel}：已选 ${countScopeIds(operatorUserIds)} 人`,
            })
        }
        if (orgUnitIds) {
            chips.push({
                key: "orgUnitIds",
                label: `组织：已选 ${countScopeIds(orgUnitIds)} 个${includeDescendants ? "（含下级）" : ""}`,
            })
        }
        if (supplierId) chips.push({ key: "supplierId", label: "已限定供应商" })
        if (purchaseOrderId)
            chips.push({ key: "purchaseOrderId", label: "已限定采购单" })
        if (payableAccountId) {
            chips.push({ key: "payableAccountId", label: "已限定应付记录" })
        }
        return chips
    }, [
        includeDescendants,
        operatorLabel,
        operatorUserIds,
        orgUnitIds,
        payableAccountId,
        procurementOwnerUserIds,
        purchaseOrderId,
        qParam,
        supplierId,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        procurementOwnerDraft.trim() !== (procurementOwnerUserIds ?? "") ||
        operatorDraft.trim() !== (operatorUserIds ?? "") ||
        orgDraft.trim() !== (orgUnitIds ?? "") ||
        descendantsDraft !== includeDescendants

    React.useEffect(() => {
        setProcurementOwnerDraft(procurementOwnerUserIds ?? "")
        setOperatorDraft(operatorUserIds ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
    }, [
        includeDescendants,
        operatorUserIds,
        orgUnitIds,
        procurementOwnerUserIds,
    ])

    return {
        canUseOwner,
        canUseBusiness,
        canUseOrg,
        view,
        operatorLabel,
        qParam,
        procurementOwnerUserIds,
        operatorUserIds,
        orgUnitIds,
        includeDescendants,
        supplierId,
        purchaseOrderId,
        payableAccountId,
        query,
        pageFromUrl,
        pagination,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        procurementOwnerDraft,
        setProcurementOwnerDraft,
        operatorDraft,
        setOperatorDraft,
        orgDraft,
        setOrgDraft,
        descendantsDraft,
        setDescendantsDraft,
        panelOpen,
        setPanelOpen,
        hasActiveFilters,
        hasPendingChanges,
        appliedChips,
        patchUrl,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
        handlePaginationChange,
        changeView,
    }
}
