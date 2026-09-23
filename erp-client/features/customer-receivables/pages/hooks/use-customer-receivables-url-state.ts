"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import type {
    CustomerAccountsQuery,
    CustomerAccountsView,
    CustomerReceivablesFilterKey,
    DueFilter,
    ReceivableStatusFilter,
} from "@/features/customer-receivables/types"
import { parseDue, parseView } from "../lib/url-params"

export type CustomerReceivablesPatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

export interface CustomerReceivablesUrlState {
    view: CustomerAccountsView
    qParam: string
    counterpartyPartyId: string | undefined
    customerId: string | undefined
    due: DueFilter | undefined
    status: Exclude<ReceivableStatusFilter, "all"> | undefined
    focusId: string | undefined
    salesOrderId: string | undefined
    registerMode: "receipt" | "invoice" | undefined
    receivableAccountId: string | undefined
    returnTo: string | undefined
    from: string | undefined
    sessionId: string | undefined
    previewKind:
        | "receivable"
        | "receipt"
        | "invoice"
        | "refund"
        | "reversal"
        | null
    previewId: string | undefined
    workItemId: string | undefined
    query: CustomerAccountsQuery
    pageFromUrl: number
    pagination: PaginationState
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    counterpartyPartyIdDraft: string | null
    setCounterpartyPartyIdDraft: React.Dispatch<
        React.SetStateAction<string | null>
    >
    dueDraft: DueFilter
    setDueDraft: React.Dispatch<React.SetStateAction<DueFilter>>
    statusDraft: ReceivableStatusFilter
    setStatusDraft: React.Dispatch<React.SetStateAction<ReceivableStatusFilter>>
    panelOpen: boolean
    setPanelOpen: React.Dispatch<React.SetStateAction<boolean>>
    hasActiveFilters: boolean
    hasStructuredFilters: boolean
    hasPendingChanges: boolean
    patchUrl: CustomerReceivablesPatchUrl
    applyFilters: () => void
    removeFilter: (key: CustomerReceivablesFilterKey) => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
    clearFilters: () => void
    handlePaginationChange: (next: PaginationState) => void
}

function parseReceivableStatus(raw: string | null): ReceivableStatusFilter {
    if (raw === "open" || raw === "partial" || raw === "settled") return raw
    return "all"
}

export function useCustomerReceivablesUrlState(
    options: {
        fixedSalesOrderId?: string
        stateMode?: "url" | "local"
    } = {},
): CustomerReceivablesUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const routeSearchParams = useSearchParams()
    const [localSearchParams, setLocalSearchParams] = React.useState(
        () => new URLSearchParams(),
    )
    const localState = options.stateMode === "local"
    const searchParams = localState ? localSearchParams : routeSearchParams

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const counterpartyPartyId =
        view === "receivable"
            ? (searchParams.get("counterpartyId") ?? undefined)
            : undefined
    const customerId = searchParams.get("customerId") ?? undefined
    const due = parseDue(searchParams.get("due"))
    const statusDraftFromUrl = parseReceivableStatus(searchParams.get("status"))
    const status = statusDraftFromUrl === "all" ? undefined : statusDraftFromUrl
    const focusId = searchParams.get("focusId") ?? undefined
    const salesOrderId =
        options.fixedSalesOrderId ??
        searchParams.get("salesOrderId") ??
        undefined
    const registerParam = searchParams.get("register")
    const registerMode =
        registerParam === "invoice" || registerParam === "receipt"
            ? registerParam
            : undefined
    const receivableAccountId =
        searchParams.get("receivableAccountId") ?? undefined
    const returnTo = searchParams.get("returnTo") ?? undefined
    const from = searchParams.get("from") ?? undefined
    const sessionId = searchParams.get("sessionId") ?? undefined
    const previewKindRaw = searchParams.get("previewKind")
    const previewKind =
        previewKindRaw === "receivable" ||
        previewKindRaw === "receipt" ||
        previewKindRaw === "invoice" ||
        previewKindRaw === "refund" ||
        previewKindRaw === "reversal"
            ? previewKindRaw
            : null
    const previewId = searchParams.get("previewId") ?? undefined
    const workItemId =
        searchParams.get("currentWorkItemId") ??
        searchParams.get("workItemId") ??
        undefined

    // Draft：本地受控，提交前不触发请求（docs/ui-filter-design.md §5）。
    const [searchDraft, setSearchDraft] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [counterpartyPartyIdDraft, setCounterpartyPartyIdDraft] =
        React.useState<string | null>(counterpartyPartyId ?? null)
    const [dueDraft, setDueDraft] = React.useState<DueFilter>(due ?? "all")
    const [statusDraft, setStatusDraft] =
        React.useState<ReceivableStatusFilter>(statusDraftFromUrl)

    const hasStructuredFilters = Boolean(counterpartyPartyId || status)
    // 深链只显示已生效标签，不自动打开面板。
    const [panelOpen, setPanelOpen] = React.useState(false)

    // 分页从 URL 派生；筛选变更写 URL 并回第 1 页。
    const pageFromUrl = React.useMemo(
        () =>
            Math.max(
                1,
                Number.parseInt(searchParams.get("page") ?? "1", 10) || 1,
            ),
        [searchParams],
    )
    const pageSize = 20

    const query: CustomerAccountsQuery = React.useMemo(
        () => ({
            view,
            page: pageFromUrl,
            pageSize,
            q: qParam || undefined,
            counterpartyPartyId,
            customerId,
            due,
            status,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        }),
        [
            view,
            pageFromUrl,
            qParam,
            counterpartyPartyId,
            customerId,
            due,
            status,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        ],
    )

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize }),
        [pageFromUrl],
    )

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        patchOptions?: { replace?: boolean },
    ) {
        if (localState) {
            setLocalSearchParams((current) => {
                const next = new URLSearchParams(current.toString())
                for (const [key, value] of Object.entries(patch)) {
                    if (value == null || value === "") next.delete(key)
                    else next.set(key, value)
                }
                return next
            })
            return
        }
        // 筛选写入使用 replace + scroll:false，不膨胀历史、不跳动滚动位置。
        patchSearchParams(
            { router, pathname, searchParams: routeSearchParams, view },
            patch,
            patchOptions?.replace
                ? { replace: true, scroll: false }
                : patchOptions,
        )
    }

    const hasActiveFilters = Boolean(
        qParam.trim() ||
        counterpartyPartyId ||
        customerId ||
        (due && due !== "all") ||
        status ||
        salesOrderId ||
        receivableAccountId,
    )

    /** 单一提交路径：收起态 Enter 与主行「查询」共用。 */
    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                counterpartyId:
                    view === "receivable"
                        ? counterpartyPartyIdDraft || null
                        : null,
                due: dueDraft === "all" ? null : dueDraft,
                status: statusDraft === "all" ? null : statusDraft,
                page: null,
            },
            { replace: true },
        )
        setPanelOpen(false)
    }, [
        counterpartyPartyIdDraft,
        dueDraft,
        // eslint-disable-next-line react-hooks/exhaustive-deps
        patchUrl,
        searchDraft,
        statusDraft,
        view,
    ])

    /** 移除单个已生效条件；来源锁定参数（customerId/salesOrderId…）同样可单独移除。 */
    const removeFilter = React.useCallback(
        (key: CustomerReceivablesFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "counterpartyId") setCounterpartyPartyIdDraft(null)
            if (key === "due") setDueDraft("all")
            if (key === "status") setStatusDraft("all")
            patchUrl({ [key]: null, page: null }, { replace: true })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl],
    )

    /** 只清往来主体草稿；保留搜索、状态、到期和当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setCounterpartyPartyIdDraft(null)
    }, [])

    /** 取消、关闭、Esc 和外点只恢复往来主体草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setCounterpartyPartyIdDraft(counterpartyPartyId ?? null)
        setPanelOpen(false)
    }, [counterpartyPartyId])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        (view === "receivable" &&
            counterpartyPartyIdDraft !== (counterpartyPartyId ?? null)) ||
        dueDraft !== (due ?? "all") ||
        statusDraft !== statusDraftFromUrl

    /** 清全部筛选参数 + 分页回 1；保留 view 与导航上下文。 */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setCounterpartyPartyIdDraft(null)
        setDueDraft("all")
        setStatusDraft("all")
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                counterpartyId: null,
                customerId: null,
                due: null,
                status: null,
                salesOrderId: null,
                receivableAccountId: null,
                focusId: null,
                previewKind: null,
                previewId: null,
                page: null,
            },
            { replace: true },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchParams, pathname, view])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            patchUrl(
                {
                    page:
                        next.pageIndex + 1 > 1
                            ? String(next.pageIndex + 1)
                            : null,
                },
                { replace: true },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
    )

    // URL 回填：只同步 Draft，不重置面板展开态（§5.4 / §5.5）。
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(qParam)
        }
        setCounterpartyPartyIdDraft(counterpartyPartyId ?? null)
        setDueDraft(due ?? "all")
        setStatusDraft(statusDraftFromUrl)
    }, [counterpartyPartyId, due, qParam, statusDraftFromUrl])

    // `/` 聚焦搜索；Dialog / Sheet 打开时不得聚焦背景搜索框（§3.2、§14.4）。
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    return {
        view,
        qParam,
        counterpartyPartyId,
        customerId,
        due,
        status,
        focusId,
        salesOrderId,
        registerMode,
        receivableAccountId,
        returnTo,
        from,
        sessionId,
        previewKind,
        previewId,
        workItemId,
        query,
        pageFromUrl,
        pagination,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        counterpartyPartyIdDraft,
        setCounterpartyPartyIdDraft,
        dueDraft,
        setDueDraft,
        statusDraft,
        setStatusDraft,
        panelOpen: view === "receivable" && panelOpen,
        setPanelOpen,
        hasActiveFilters,
        hasStructuredFilters,
        hasPendingChanges,
        patchUrl,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
        handlePaginationChange,
    }
}
