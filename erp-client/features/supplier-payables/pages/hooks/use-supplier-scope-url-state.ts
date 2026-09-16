"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { countScopeIds, parseScopeIdList } from "@/lib/funds-scope"
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
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const viewParam = parseView(searchParams.get("view"))
    const view: SupplierScopeQuery["view"] =
        viewParam === "unallocated" ? "payable" : viewParam
    const qParam = searchParams.get("q") ?? ""
    const procurementOwnerUserIds = parseScopeIdList(
        searchParams.get("procurementOwnerUserIds"),
    )
    const operatorUserIds = parseScopeIdList(
        searchParams.get("operatorUserIds"),
    )
    const orgUnitIds = parseScopeIdList(searchParams.get("orgUnitIds"))
    const includeDescendants = searchParams.get("includeDescendants") === "true"
    const supplierId = searchParams.get("supplierId") ?? undefined
    const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
    const payableAccountId = searchParams.get("payableAccountId") ?? undefined
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined

    const [searchDraft, setSearchDraft] = React.useState(qParam)
    const [procurementOwnerDraft, setProcurementOwnerDraft] = React.useState(
        procurementOwnerUserIds ?? "",
    )
    const [operatorDraft, setOperatorDraft] = React.useState(
        operatorUserIds ?? "",
    )
    const [orgDraft, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraft, setDescendantsDraft] =
        React.useState(includeDescendants)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [panelOpen, setPanelOpen] = React.useState(
        Boolean(procurementOwnerUserIds || operatorUserIds || orgUnitIds),
    )

    const pageFromUrl = React.useMemo(() => {
        const page = Number.parseInt(searchParams.get("page") ?? "1", 10)
        return Number.isFinite(page) && page >= 1 ? page : 1
    }, [searchParams])
    const pageSize = 20

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

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize }),
        [pageFromUrl],
    )

    const patchUrl = React.useCallback(
        (
            patch: Record<string, string | null | undefined>,
            options?: { replace?: boolean },
        ) => {
            patchSearchParams(
                { router, pathname, searchParams, view },
                patch,
                options?.replace ? { replace: true, scroll: false } : options,
            )
        },
        [pathname, router, searchParams, view],
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
        [patchUrl],
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
    }, [patchUrl])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState, nextScopeVersion?: string) => {
            patchUrl(
                {
                    page:
                        next.pageIndex + 1 > 1
                            ? String(next.pageIndex + 1)
                            : null,
                    scopeVersion:
                        next.pageIndex + 1 > 1
                            ? (nextScopeVersion ?? scopeVersion)
                            : null,
                },
                { replace: true },
            )
        },
        [patchUrl, scopeVersion],
    )

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
                label: `经办人：已选 ${countScopeIds(operatorUserIds)} 人`,
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
            chips.push({ key: "payableAccountId", label: "已限定应付子账" })
        }
        return chips
    }, [
        includeDescendants,
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
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(qParam)
        }
        setProcurementOwnerDraft(procurementOwnerUserIds ?? "")
        setOperatorDraft(operatorUserIds ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
    }, [
        includeDescendants,
        operatorUserIds,
        orgUnitIds,
        procurementOwnerUserIds,
        qParam,
    ])

    return {
        view,
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
        clearFilters,
        handlePaginationChange,
        changeView,
    }
}
