"use client"

import * as React from "react"
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

type View = ReceivableScopeQuery["view"]

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
    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const salesOwnerUserIds = parseScopeIdList(
        searchParams.get("salesOwnerUserIds"),
    )
    const operatorUserIds = parseScopeIdList(
        searchParams.get("operatorUserIds"),
    )
    const operatorKind = parseOperatorKind(searchParams.get("operatorKind"))
    const orgUnitIds = parseScopeIdList(searchParams.get("orgUnitIds"))
    const includeDescendants = searchParams.get("includeDescendants") === "true"
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const receivableAccountId =
        searchParams.get("receivableAccountId") ?? undefined
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined
    const { pageFromUrl, pagination, patchUrl, handlePaginationChange } =
        useScopeUrlPagination({ view, pageSize, scopeVersion })

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const { searchDraft, setSearchDraft } = useSearchDraft(qParam, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })
    const [salesOwnerDraft, setSalesOwnerDraft] = React.useState(
        salesOwnerUserIds ?? "",
    )
    const [operatorDraft, setOperatorDraft] = React.useState(
        operatorUserIds ?? "",
    )
    const [operatorKindDraft, setOperatorKindDraft] = React.useState<
        "register" | "settle" | ""
    >(operatorKind ?? "")
    const [orgDraft, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraft, setDescendantsDraft] =
        React.useState(includeDescendants)
    const [panelOpen, setPanelOpen] = React.useState(false)

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
                        ? operatorKindDraft || null
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

    /** 取消、关闭、Esc 和外点只恢复低频草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setOperatorDraft(operatorUserIds ?? "")
        setOperatorKindDraft(operatorKind ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [includeDescendants, operatorKind, operatorUserIds, orgUnitIds])

    const changeView = React.useCallback(
        (nextView: View) => {
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
        [patchUrl],
    )

    const appliedChips = React.useMemo<readonly ReceivableScopeChip[]>(() => {
        const chips: ReceivableScopeChip[] = []
        const queryText = qParam.trim()
        if (queryText) chips.push({ key: "q", label: `搜索：${queryText}` })
        if (salesOwnerUserIds) {
            chips.push({
                key: "salesOwnerUserIds",
                label: `负责销售：已选 ${countScopeIds(salesOwnerUserIds)} 人`,
            })
        }
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
        qParam,
        receivableAccountId,
        salesOrderId,
        salesOwnerUserIds,
        view,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        salesOwnerDraft.trim() !== (salesOwnerUserIds ?? "") ||
        operatorDraft.trim() !== (operatorUserIds ?? "") ||
        (view === "receipt" && operatorDraft.trim()
            ? operatorKindDraft !== (operatorKind ?? "")
            : false) ||
        orgDraft.trim() !== (orgUnitIds ?? "") ||
        descendantsDraft !== includeDescendants

    React.useEffect(() => {
        setSalesOwnerDraft(salesOwnerUserIds ?? "")
        setOperatorDraft(operatorUserIds ?? "")
        setOperatorKindDraft(operatorKind ?? "")
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
    }, [
        includeDescendants,
        operatorKind,
        operatorUserIds,
        orgUnitIds,
        salesOwnerUserIds,
    ])

    return {
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
