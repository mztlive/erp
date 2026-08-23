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
    ReceivableReviewStatusFilter,
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
    reviewStatus: Exclude<ReceivableReviewStatusFilter, "all"> | undefined
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
    setCounterpartyPartyIdDraft: React.Dispatch<React.SetStateAction<string | null>>
    dueDraft: DueFilter
    setDueDraft: React.Dispatch<React.SetStateAction<DueFilter>>
    statusDraft: ReceivableStatusFilter
    setStatusDraft: React.Dispatch<React.SetStateAction<ReceivableStatusFilter>>
    reviewStatusDraft: ReceivableReviewStatusFilter
    setReviewStatusDraft: React.Dispatch<
        React.SetStateAction<ReceivableReviewStatusFilter>
    >
    panelOpen: boolean
    setPanelOpen: React.Dispatch<React.SetStateAction<boolean>>
    hasActiveFilters: boolean
    hasStructuredFilters: boolean
    patchUrl: CustomerReceivablesPatchUrl
    applyFilters: () => void
    removeFilter: (key: CustomerReceivablesFilterKey) => void
    resetMoreFilters: () => void
    clearFilters: () => void
    handlePaginationChange: (next: PaginationState) => void
}

function parseReceivableStatus(
    raw: string | null,
): ReceivableStatusFilter {
    if (raw === "open" || raw === "partial" || raw === "settled") return raw
    return "all"
}

function parseReviewStatus(
    raw: string | null,
): ReceivableReviewStatusFilter {
    if (
        raw === "pending_opening" ||
        raw === "reviewed" ||
        raw === "pending_sync_diff"
    ) {
        return raw
    }
    return "all"
}

export function useCustomerReceivablesUrlState(): CustomerReceivablesUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const counterpartyPartyId = searchParams.get("counterpartyId") ?? undefined
    const customerId = searchParams.get("customerId") ?? undefined
    const due = parseDue(searchParams.get("due"))
    const statusDraftFromUrl = parseReceivableStatus(searchParams.get("status"))
    const status = statusDraftFromUrl === "all" ? undefined : statusDraftFromUrl
    const reviewStatusDraftFromUrl = parseReviewStatus(
        searchParams.get("reviewStatus"),
    )
    const reviewStatus =
        reviewStatusDraftFromUrl === "all"
            ? undefined
            : reviewStatusDraftFromUrl
    const focusId = searchParams.get("focusId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
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
    const [reviewStatusDraft, setReviewStatusDraft] =
        React.useState<ReceivableReviewStatusFilter>(reviewStatusDraftFromUrl)

    const hasStructuredFilters = Boolean(
        counterpartyPartyId ||
            (due && due !== "all") ||
            status ||
            reviewStatus,
    )
    // 有结构化条件的初始深链展开面板；后续 URL 回填不得抢夺展开态。
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)

    const query: CustomerAccountsQuery = React.useMemo(
        () => ({
            view,
            q: qParam || undefined,
            counterpartyPartyId,
            customerId,
            due,
            status,
            reviewStatus,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        }),
        [
            view,
            qParam,
            counterpartyPartyId,
            customerId,
            due,
            status,
            reviewStatus,
            salesOrderId,
            receivableAccountId,
            returnTo,
            from,
        ],
    )

    // 分页从 URL 派生（P6）；筛选变更写 URL 并回第 1 页。
    const pageFromUrl = React.useMemo(
        () =>
            Math.max(
                1,
                Number.parseInt(searchParams.get("page") ?? "1", 10) || 1,
            ),
        [searchParams],
    )
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize: 20 }),
        [pageFromUrl],
    )

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        // 筛选写入使用 replace + scroll:false，不膨胀历史、不跳动滚动位置。
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options?.replace
                ? { replace: true, scroll: false }
                : options,
        )
    }

    const hasActiveFilters = Boolean(
        qParam.trim() ||
            counterpartyPartyId ||
            customerId ||
            (due && due !== "all") ||
            status ||
            reviewStatus ||
            salesOrderId ||
            receivableAccountId,
    )

    /** 单一提交路径：收起态 Enter 与展开态「应用全部筛选」都走这里。 */
    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                counterpartyId: counterpartyPartyIdDraft || null,
                due: dueDraft === "all" ? null : dueDraft,
                status: statusDraft === "all" ? null : statusDraft,
                reviewStatus:
                    reviewStatusDraft === "all" ? null : reviewStatusDraft,
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
        reviewStatusDraft,
        searchDraft,
        statusDraft,
    ])

    /** 移除单个已生效条件；来源锁定参数（customerId/salesOrderId…）同样可单独移除。 */
    const removeFilter = React.useCallback(
        (key: CustomerReceivablesFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "counterpartyId") setCounterpartyPartyIdDraft(null)
            if (key === "due") setDueDraft("all")
            if (key === "status") setStatusDraft("all")
            if (key === "reviewStatus") setReviewStatusDraft("all")
            patchUrl({ [key]: null, page: null }, { replace: true })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl],
    )

    /** 只清结构化条件；保留关键词和来源锁定，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setCounterpartyPartyIdDraft(null)
        setDueDraft("all")
        setStatusDraft("all")
        setReviewStatusDraft("all")
        patchUrl(
            {
                counterpartyId: null,
                due: null,
                status: null,
                reviewStatus: null,
                page: null,
            },
            { replace: true },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 清全部筛选参数 + 分页回 1；保留 view 与导航上下文。 */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setCounterpartyPartyIdDraft(null)
        setDueDraft("all")
        setStatusDraft("all")
        setReviewStatusDraft("all")
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                counterpartyId: null,
                customerId: null,
                due: null,
                status: null,
                reviewStatus: null,
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
                        next.pageIndex + 1 > 1 ? String(next.pageIndex + 1) : null,
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
        setReviewStatusDraft(reviewStatusDraftFromUrl)
    }, [
        counterpartyPartyId,
        due,
        qParam,
        reviewStatusDraftFromUrl,
        statusDraftFromUrl,
    ])

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
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"]',
                )
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
        reviewStatus,
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
        reviewStatusDraft,
        setReviewStatusDraft,
        panelOpen,
        setPanelOpen,
        hasActiveFilters,
        hasStructuredFilters,
        patchUrl,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearFilters,
        handlePaginationChange,
    }
}
