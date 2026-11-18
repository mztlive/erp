"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSearchParams } from "@/lib/navigation"

import { countScopeIds, parseScopeIdList } from "@/lib/funds-scope"
import { useScopeUrlPagination } from "@/lib/use-scope-url-pagination"
import { useSearchDraft } from "@/lib/use-search-draft"
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
    const pageSize = 10
    const searchParams = useSearchParams()
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canUseOwner = hasPermission(permissions, "sales_person:list")
    const canUseBusiness = hasPermission(permissions, "business_person:list")
    const canUseOrg = hasPermission(permissions, "org_unit:list")
    const unavailableFilters = React.useMemo(
        () =>
            Object.fromEntries(
                [
                    [canUseOwner, "salesOwnerUserIds"] as const,
                    [canUseBusiness, "applicantUserIds"] as const,
                    [canUseBusiness, "handlerUserIds"] as const,
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

    const qParam = searchParams.get("q") ?? ""
    const salesOwnerUserIds = canUseOwner
        ? parseScopeIdList(searchParams.get("salesOwnerUserIds"))
        : undefined
    const applicantUserIds = canUseBusiness
        ? parseScopeIdList(searchParams.get("applicantUserIds"))
        : undefined
    const handlerUserIds = canUseBusiness
        ? parseScopeIdList(searchParams.get("handlerUserIds"))
        : undefined
    const orgUnitIds = canUseOrg
        ? parseScopeIdList(searchParams.get("orgUnitIds"))
        : undefined
    const includeDescendants =
        canUseOrg && searchParams.get("includeDescendants") === "true"
    const status = searchParams.get("status") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const scopeVersion = hasUnavailableFilters
        ? undefined
        : (searchParams.get("scopeVersion") ?? undefined)
    const {
        pageFromUrl: rawPage,
        pagination: rawPagination,
        patchUrl: patchPaginationUrl,
        handlePaginationChange,
    } = useScopeUrlPagination({ pageSize, scopeVersion })

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
    const [salesOwnerDraftValue, setSalesOwnerDraft] = React.useState(
        salesOwnerUserIds ?? "",
    )
    const [applicantDraftValue, setApplicantDraft] = React.useState(
        applicantUserIds ?? "",
    )
    const [handlerDraftValue, setHandlerDraft] = React.useState(
        handlerUserIds ?? "",
    )
    const [orgDraftValue, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraftValue, setDescendantsDraft] =
        React.useState(includeDescendants)
    const [panelOpen, setPanelOpen] = React.useState(false)

    const salesOwnerDraft = canUseOwner ? salesOwnerDraftValue : ""
    const applicantDraft = canUseBusiness ? applicantDraftValue : ""
    const handlerDraft = canUseBusiness ? handlerDraftValue : ""
    const orgDraft = canUseOrg ? orgDraftValue : ""
    const descendantsDraft = canUseOrg ? descendantsDraftValue : false
    React.useEffect(() => {
        if (!canUseOwner) setSalesOwnerDraft("")
        if (!canUseBusiness) setApplicantDraft("")
        if (!canUseBusiness) setHandlerDraft("")
        if (!canUseOrg) setOrgDraft("")
        if (!canUseOrg) setDescendantsDraft(false)
    }, [canUseOwner, canUseBusiness, canUseOrg])

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
        [patchUrl, setSearchDraft],
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
    }, [patchUrl, setSearchDraft])

    /** 只清申请人、当前开票处理人和业务组织草稿；保留搜索与负责销售。 */
    const resetMoreFilters = React.useCallback(() => {
        setApplicantDraft("")
        setHandlerDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
    }, [])

    /** 取消、关闭、Esc 和外点只恢复低频草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setApplicantDraft(applicantUserIds ?? "")
        setHandlerDraft(handlerUserIds ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [applicantUserIds, handlerUserIds, includeDescendants, orgUnitIds])

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
        salesOwnerUserIds,
    ])

    return {
        canUseOwner,
        canUseBusiness,
        canUseOrg,
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
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
        handlePaginationChange,
    }
}
