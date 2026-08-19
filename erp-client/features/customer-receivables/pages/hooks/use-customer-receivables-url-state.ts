"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import type {
    CustomerAccountsQuery,
    CustomerAccountsView,
    DueFilter,
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
    status: string | undefined
    reviewStatus: string | undefined
    focusId: string | undefined
    salesOrderId: string | undefined
    receivableAccountId: string | undefined
    returnTo: string | undefined
    from: string | undefined
    sessionId: string | undefined
    previewKind: "receivable" | "receipt" | "invoice" | "refund" | null
    previewId: string | undefined
    workItemId: string | undefined
    query: CustomerAccountsQuery
    pageFromUrl: number
    pagination: PaginationState
    searchInput: string
    setSearchInput: React.Dispatch<React.SetStateAction<string>>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    hasActiveFilters: boolean
    patchUrl: CustomerReceivablesPatchUrl
    clearFilters: () => void
    handlePaginationChange: (next: PaginationState) => void
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
    const status = searchParams.get("status") ?? undefined
    const reviewStatus = searchParams.get("reviewStatus") ?? undefined
    const focusId = searchParams.get("focusId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
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
        previewKindRaw === "refund"
            ? previewKindRaw
            : null
    const previewId = searchParams.get("previewId") ?? undefined
    const workItemId =
        searchParams.get("currentWorkItemId") ??
        searchParams.get("workItemId") ??
        undefined

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

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
        patchSearchParams({ router, pathname, searchParams, view }, patch, options)
    }

    const hasActiveFilters = Boolean(
        qParam.trim() ||
            counterpartyPartyId ||
            customerId ||
            due ||
            status ||
            reviewStatus ||
            salesOrderId ||
            receivableAccountId,
    )

    /** P4：清全部筛选参数 + 分页回 1；保留 view/导航上下文。 */
    const clearFilters = React.useCallback(() => {
        setSearchInput("")
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

    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl(
                { q: searchInput.trim() || null, page: null },
                { replace: true },
            )
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

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
        searchInput,
        setSearchInput,
        searchInputRef,
        hasActiveFilters,
        patchUrl,
        clearFilters,
        handlePaginationChange,
    }
}
