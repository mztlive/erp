"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSearchParams } from "@/lib/navigation"

import { countScopeIds, parseScopeIdList } from "@/lib/funds-scope"
import { useScopeUrlPagination } from "@/lib/use-scope-url-pagination"
import { useSearchDraft } from "@/lib/use-search-draft"
import type { ReceivableScopeQuery } from "@/features/customer-receivables/api/scoped"

export type ReceivableScopeFilterKey =
    | "q"
    | "salesOwnerUserIds"
    | "operatorUserIds"
    | "operatorKind"
    | "orgUnitIds"
    | "salesOrderId"
    | "receivableAccountId"

export type ReceivableScopeChip = Readonly<{
    key: ReceivableScopeFilterKey
    label: string
}>

const VIEW_RESOURCE = {
    receivable: "receivable_account",
    receipt: "customer_receipt",
    sales_invoice: "invoice",
} as const satisfies Partial<Record<ReceivableScopeQuery["view"], string>>

const RECEIVABLE_SCOPE_VIEWS = [
    "receivable",
    "receipt",
    "sales_invoice",
] as const

type View = keyof typeof VIEW_RESOURCE

function parseView(raw: string | null): View {
    if (raw === "receipt" || raw === "sales_invoice") return raw
    return "receivable"
}

function parseOperatorKind(
    raw: string | null,
): "register" | "settle" | undefined {
    if (raw === "register" || raw === "settle") return raw
    return undefined
}

/** 客户往来范围查询 URL 状态：有效条件进入 URL 与 Query key。 */
export function useReceivableScopeUrlState() {
    const pageSize = 20
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
                    [canUseBusiness, "operatorUserIds"] as const,
                    [canUseBusiness, "operatorKind"] as const,
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

    const view = parseView(searchParams.get("view"))
    const visibleViews = RECEIVABLE_SCOPE_VIEWS.filter((item) =>
        hasPermission(permissions, `${VIEW_RESOURCE[item]}:list`),
    )
    const canReadView = visibleViews.includes(view)
    const canPreview = hasPermission(
        permissions,
        `${VIEW_RESOURCE[view]}:detail`,
    )
    const qParam = searchParams.get("q") ?? ""
    const salesOwnerUserIds = canUseOwner
        ? parseScopeIdList(searchParams.get("salesOwnerUserIds"))
        : undefined
    const operatorUserIds = canUseBusiness
        ? parseScopeIdList(searchParams.get("operatorUserIds"))
        : undefined
    // 回款接口要求经办人查询同时指定登记或核销，旧链接未指定时按登记查询。
    const operatorKind =
        canUseBusiness && view === "receipt"
            ? (parseOperatorKind(searchParams.get("operatorKind")) ??
              (operatorUserIds ? "register" : undefined))
            : undefined
    const orgUnitIds = canUseOrg
        ? parseScopeIdList(searchParams.get("orgUnitIds"))
        : undefined
    const includeDescendants =
        Boolean(orgUnitIds) && searchParams.get("includeDescendants") === "true"
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const receivableAccountId =
        searchParams.get("receivableAccountId") ?? undefined
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
        if (account.isSuccess && hasUnavailableFilters) {
            patchUrl({ scopeVersion: null, page: null }, { replace: true })
        }
    }, [account.isSuccess, hasUnavailableFilters, patchUrl])

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const { searchDraft, setSearchDraft } = useSearchDraft(qParam, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })
    const [salesOwnerDraftValue, setSalesOwnerDraft] = React.useState(
        salesOwnerUserIds ?? "",
    )
    const [operatorDraftValue, setOperatorDraft] = React.useState(
        operatorUserIds ?? "",
    )
    const [operatorKindDraftValue, setOperatorKindDraft] = React.useState<
        "register" | "settle" | ""
    >(operatorKind ?? "")
    const [orgDraftValue, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraftValue, setDescendantsDraft] =
        React.useState(includeDescendants)
    const [panelOpen, setPanelOpen] = React.useState(false)

    const salesOwnerDraft = canUseOwner ? salesOwnerDraftValue : ""
    const operatorDraft = canUseBusiness ? operatorDraftValue : ""
    const operatorKindDraft = canUseBusiness ? operatorKindDraftValue : ""
    const orgDraft = canUseOrg ? orgDraftValue : ""
    const descendantsDraft = canUseOrg ? descendantsDraftValue : false

    const query: ReceivableScopeQuery = React.useMemo(
        () => ({
            view,
            page: pageFromUrl,
            pageSize,
            scopeVersion,
            salesOwnerUserIds,
            operatorUserIds,
            operatorKind,
            orgUnitIds,
            includeDescendants:
                orgUnitIds && includeDescendants ? true : undefined,
            q: qParam || undefined,
            salesOrderId,
            receivableAccountId,
        }),
        [
            includeDescendants,
            operatorKind,
            operatorUserIds,
            orgUnitIds,
            pageFromUrl,
            qParam,
            receivableAccountId,
            salesOrderId,
            salesOwnerUserIds,
            scopeVersion,
            view,
        ],
    )

    const hasActiveFilters = Boolean(
        qParam.trim() ||
        salesOwnerUserIds ||
        operatorUserIds ||
        orgUnitIds ||
        salesOrderId ||
        receivableAccountId,
    )

    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                salesOwnerUserIds: salesOwnerDraft.trim() || null,
                operatorUserIds: operatorDraft.trim() || null,
                operatorKind:
                    view === "receipt" && operatorDraft.trim()
                        ? operatorKindDraft || "register"
                        : null,
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
        operatorKindDraft,
        orgDraft,
        patchUrl,
        salesOwnerDraft,
        searchDraft,
        view,
    ])

    const removeFilter = React.useCallback(
        (key: ReceivableScopeFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "salesOwnerUserIds") setSalesOwnerDraft("")
            if (key === "operatorUserIds") {
                setOperatorDraft("")
                setOperatorKindDraft("")
            }
            if (key === "operatorKind") setOperatorKindDraft("")
            if (key === "orgUnitIds") {
                setOrgDraft("")
                setDescendantsDraft(false)
            }
            const patch: Record<string, string | null | undefined> = {
                [key]: null,
                scopeVersion: null,
                page: null,
            }
            if (key === "operatorUserIds") patch.operatorKind = null
            if (key === "orgUnitIds") patch.includeDescendants = null
            patchUrl(patch, { replace: true })
        },
        [patchUrl, setSearchDraft],
    )

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setSalesOwnerDraft("")
        setOperatorDraft("")
        setOperatorKindDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                salesOwnerUserIds: null,
                operatorUserIds: null,
                operatorKind: null,
                orgUnitIds: null,
                includeDescendants: null,
                salesOrderId: null,
                receivableAccountId: null,
                scopeVersion: null,
                page: null,
            },
            { replace: true },
        )
    }, [patchUrl, setSearchDraft])

    /** 只清经办人、经办人口径和组织草稿；保留搜索与负责销售。 */
    const resetMoreFilters = React.useCallback(() => {
        setOperatorDraft("")
        setOperatorKindDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
    }, [])

    /** 收起或按 Esc 时只恢复低频草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setOperatorDraft(operatorUserIds ?? "")
        setOperatorKindDraft(operatorKind ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [includeDescendants, operatorKind, operatorUserIds, orgUnitIds])

    const changeView = React.useCallback(
        (nextView: View) => {
            cancelMoreFilters()
            patchUrl(
                {
                    view: nextView,
                    operatorKind: null,
                    scopeVersion: null,
                    page: null,
                },
                { replace: true },
            )
        },
        [cancelMoreFilters, patchUrl],
    )

    const appliedChips = React.useMemo<readonly ReceivableScopeChip[]>(() => {
        const chips: ReceivableScopeChip[] = []
        if (operatorUserIds) {
            chips.push({
                key: "operatorUserIds",
                label: `经办人：已选 ${countScopeIds(operatorUserIds)} 人${
                    view === "receipt" && operatorKind
                        ? operatorKind === "register"
                            ? "（登记）"
                            : "（核销）"
                        : ""
                }`,
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
        if (receivableAccountId) {
            chips.push({ key: "receivableAccountId", label: "已限定应收款项" })
        }
        return chips
    }, [
        includeDescendants,
        operatorKind,
        operatorUserIds,
        orgUnitIds,
        receivableAccountId,
        salesOrderId,
        view,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        salesOwnerDraft.trim() !== (salesOwnerUserIds ?? "") ||
        operatorDraft.trim() !== (operatorUserIds ?? "") ||
        (view === "receipt" && operatorDraft.trim()
            ? (operatorKindDraft || "register") !== operatorKind
            : false) ||
        orgDraft.trim() !== (orgUnitIds ?? "") ||
        descendantsDraft !== includeDescendants

    React.useEffect(() => {
        setSalesOwnerDraft(salesOwnerUserIds ?? "")
    }, [canUseOwner, salesOwnerUserIds])
    React.useEffect(() => {
        setOperatorDraft(operatorUserIds ?? "")
        setOperatorKindDraft(operatorKind ?? "")
    }, [canUseBusiness, operatorKind, operatorUserIds])
    React.useEffect(() => {
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
    }, [canUseOrg, includeDescendants, orgUnitIds])

    return {
        account,
        canReadView,
        canPreview,
        visibleViews,
        canUseOwner,
        canUseBusiness,
        canUseOrg,
        view,
        qParam,
        salesOwnerUserIds,
        operatorUserIds,
        operatorKind,
        orgUnitIds,
        includeDescendants,
        salesOrderId,
        receivableAccountId,
        query,
        pageFromUrl,
        pagination,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        salesOwnerDraft,
        setSalesOwnerDraft,
        operatorDraft,
        setOperatorDraft,
        operatorKindDraft,
        setOperatorKindDraft,
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
