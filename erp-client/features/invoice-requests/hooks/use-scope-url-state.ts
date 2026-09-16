"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { countScopeIds, parseScopeIdList } from "@/lib/funds-scope"
import type { InvoiceRequestScopeQuery } from "@/features/invoice-requests/scoped"

export type InvoiceRequestScopeFilterKey =
    | "q"
    | "salesOwnerUserIds"
    | "applicantUserIds"
    | "handlerUserIds"
    | "orgUnitIds"
    | "salesOrderId"

export type InvoiceRequestScopeChip = Readonly<{
    key: InvoiceRequestScopeFilterKey
    label: string
}>

/** 开票申请范围查询 URL 状态：有效条件进入 URL 与 Query key。 */
export function useInvoiceRequestScopeUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const qParam = searchParams.get("q") ?? ""
    const salesOwnerUserIds = parseScopeIdList(
        searchParams.get("salesOwnerUserIds"),
    )
    const applicantUserIds = parseScopeIdList(
        searchParams.get("applicantUserIds"),
    )
    const handlerUserIds = parseScopeIdList(searchParams.get("handlerUserIds"))
    const orgUnitIds = parseScopeIdList(searchParams.get("orgUnitIds"))
    const includeDescendants = searchParams.get("includeDescendants") === "true"
    const status = searchParams.get("status") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined

    const [searchDraft, setSearchDraft] = React.useState(qParam)
    const [salesOwnerDraft, setSalesOwnerDraft] = React.useState(
        salesOwnerUserIds ?? "",
    )
    const [applicantDraft, setApplicantDraft] = React.useState(
        applicantUserIds ?? "",
    )
    const [handlerDraft, setHandlerDraft] = React.useState(handlerUserIds ?? "")
    const [orgDraft, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraft, setDescendantsDraft] =
        React.useState(includeDescendants)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [panelOpen, setPanelOpen] = React.useState(
        Boolean(
            salesOwnerUserIds ||
            applicantUserIds ||
            handlerUserIds ||
            orgUnitIds,
        ),
    )

    const pageFromUrl = React.useMemo(() => {
        const page = Number.parseInt(searchParams.get("page") ?? "1", 10)
        return Number.isFinite(page) && page >= 1 ? page : 1
    }, [searchParams])
    const pageSize = 10

    const query: InvoiceRequestScopeQuery = React.useMemo(
        () => ({
            page: pageFromUrl,
            pageSize,
            scopeVersion,
            salesOwnerUserIds,
            applicantUserIds,
            handlerUserIds,
            orgUnitIds,
            includeDescendants:
                orgUnitIds && includeDescendants ? true : undefined,
            q: qParam || undefined,
            status,
            salesOrderId,
        }),
        [
            applicantUserIds,
            handlerUserIds,
            includeDescendants,
            orgUnitIds,
            pageFromUrl,
            qParam,
            salesOrderId,
            salesOwnerUserIds,
            scopeVersion,
            status,
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
                { router, pathname, searchParams },
                patch,
                options?.replace ? { replace: true, scroll: false } : options,
            )
        },
        [pathname, router, searchParams],
    )

    const hasActiveFilters = Boolean(
        qParam.trim() ||
        salesOwnerUserIds ||
        applicantUserIds ||
        handlerUserIds ||
        orgUnitIds ||
        status ||
        salesOrderId,
    )

    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                salesOwnerUserIds: salesOwnerDraft.trim() || null,
                applicantUserIds: applicantDraft.trim() || null,
                handlerUserIds: handlerDraft.trim() || null,
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
        applicantDraft,
        descendantsDraft,
        handlerDraft,
        orgDraft,
        patchUrl,
        salesOwnerDraft,
        searchDraft,
    ])

    const removeFilter = React.useCallback(
        (key: InvoiceRequestScopeFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "salesOwnerUserIds") setSalesOwnerDraft("")
            if (key === "applicantUserIds") setApplicantDraft("")
            if (key === "handlerUserIds") setHandlerDraft("")
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
        setSalesOwnerDraft("")
        setApplicantDraft("")
        setHandlerDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                salesOwnerUserIds: null,
                applicantUserIds: null,
                handlerUserIds: null,
                orgUnitIds: null,
                includeDescendants: null,
                status: null,
                salesOrderId: null,
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

    const appliedChips = React.useMemo<
        readonly InvoiceRequestScopeChip[]
    >(() => {
        const chips: InvoiceRequestScopeChip[] = []
        const queryText = qParam.trim()
        if (queryText) chips.push({ key: "q", label: `搜索：${queryText}` })
        if (salesOwnerUserIds) {
            chips.push({
                key: "salesOwnerUserIds",
                label: `负责销售：已选 ${countScopeIds(salesOwnerUserIds)} 人`,
            })
        }
        if (applicantUserIds) {
            chips.push({
                key: "applicantUserIds",
                label: `申请人：已选 ${countScopeIds(applicantUserIds)} 人`,
            })
        }
        if (handlerUserIds) {
            chips.push({
                key: "handlerUserIds",
                label: `当前处理人：已选 ${countScopeIds(handlerUserIds)} 人`,
            })
        }
        if (orgUnitIds) {
            chips.push({
                key: "orgUnitIds",
                label: `组织：已选 ${countScopeIds(orgUnitIds)} 个${includeDescendants ? "（含下级）" : ""}`,
            })
        }
        if (salesOrderId)
            chips.push({ key: "salesOrderId", label: "已限定销售单" })
        return chips
    }, [
        applicantUserIds,
        handlerUserIds,
        includeDescendants,
        orgUnitIds,
        qParam,
        salesOrderId,
        salesOwnerUserIds,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        salesOwnerDraft.trim() !== (salesOwnerUserIds ?? "") ||
        applicantDraft.trim() !== (applicantUserIds ?? "") ||
        handlerDraft.trim() !== (handlerUserIds ?? "") ||
        orgDraft.trim() !== (orgUnitIds ?? "") ||
        descendantsDraft !== includeDescendants

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(qParam)
        }
        setSalesOwnerDraft(salesOwnerUserIds ?? "")
        setApplicantDraft(applicantUserIds ?? "")
        setHandlerDraft(handlerUserIds ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
    }, [
        applicantUserIds,
        handlerUserIds,
        includeDescendants,
        orgUnitIds,
        qParam,
        salesOwnerUserIds,
    ])

    return {
        qParam,
        salesOwnerUserIds,
        applicantUserIds,
        handlerUserIds,
        orgUnitIds,
        includeDescendants,
        status,
        salesOrderId,
        query,
        pageFromUrl,
        pagination,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        salesOwnerDraft,
        setSalesOwnerDraft,
        applicantDraft,
        setApplicantDraft,
        handlerDraft,
        setHandlerDraft,
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
    }
}
