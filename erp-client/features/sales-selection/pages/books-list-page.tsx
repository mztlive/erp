"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { PackageSearchIcon } from "lucide-react"
import type { PaginationState } from "@tanstack/react-table"

import { ListWorkspacePage } from "@/components/business/list-workspace"
import { BooksListFilterBar } from "@/features/sales-selection/components/books-list-filter-bar"
import { BooksListTable } from "@/features/sales-selection/components/books-list-table"
import { BookCopyDialog } from "@/features/sales-selection/components/book-copy-dialog"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { useBooks } from "@/features/sales-selection/hooks/queries"
import {
    BOOK_STATUS_VIEWS,
    bookIdentity,
} from "@/features/sales-selection/lib/presentation"
import type {
    BookListQuery,
    SelectionBook,
} from "@/features/sales-selection/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { booksListStyles } from "./books-list-styles"

const DEFAULT_PAGE_SIZE = 20
const POOL_HREF = "/master-data/sellable-items"
const EMPTY_BOOKS: SelectionBook[] = []

function queryFromSearchParams(searchParams: URLSearchParams): BookListQuery {
    const selectionForm = searchParams.get("selection_form")
    const status = searchParams.get("status")
    const page = Number(searchParams.get("page") ?? "1")
    const pageSize = Number(searchParams.get("page_size") ?? "")
    return {
        q: searchParams.get("q") || undefined,
        selection_form:
            selectionForm === "SINGLE_SKU" || selectionForm === "PACKAGE"
                ? selectionForm
                : "ALL",
        status: status || "ALL",
        owner_user_ids: searchParams.get("owner_user_ids") || undefined,
        org_unit_ids: searchParams.get("org_unit_ids") || undefined,
        include_descendants:
            searchParams.get("include_descendants") === "1" ? true : undefined,
        page: Number.isFinite(page) && page > 0 ? Math.floor(page) : 1,
        page_size:
            Number.isFinite(pageSize) && pageSize > 0
                ? Math.floor(pageSize)
                : DEFAULT_PAGE_SIZE,
    } as BookListQuery
}

function hasActiveQueryFilters(query: BookListQuery): boolean {
    return Boolean(
        query.q ||
        (query.selection_form && query.selection_form !== "ALL") ||
        query.owner_user_ids ||
        query.org_unit_ids,
    )
}

export const BooksListPage = () => {
    const router = useRouter()
    const searchInputRef = React.useRef<HTMLInputElement>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    useSearchHotkey(searchInputRef)
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canCreate = hasPermission(
        permissions,
        "sales_selection_booklet:create",
    )
    const canOpen = hasPermission(permissions, "sales_selection_booklet:get")
    const canCopy = canCreate && canOpen
    const canStartFromPool =
        canCreate && hasPermission(permissions, "sellable_sku:list")
    const canOpenProposal = hasPermission(
        permissions,
        "sales_selection_proposal:get",
    )
    const canFilterOwners = hasPermission(permissions, "sales_person:list")
    const canFilterOrgs = hasPermission(permissions, "org_unit:list")
    const [copySource, setCopySource] = React.useState<SelectionBook | null>(
        null,
    )
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const stripFilters = React.useCallback(
        (params: URLSearchParams) => {
            const keys = [
                ...(!canFilterOwners ? ["owner_user_ids"] : []),
                ...(!canFilterOrgs
                    ? ["org_unit_ids", "include_descendants"]
                    : []),
            ]
            const changed = keys.some((key) => params.has(key))
            for (const key of keys) params.delete(key)
            if (changed) params.delete("page")
            const orphanedDescendants =
                !params.get("org_unit_ids") && params.has("include_descendants")
            if (orphanedDescendants) params.delete("include_descendants")
            return changed || orphanedDescendants
        },
        [canFilterOwners, canFilterOrgs],
    )
    const query = React.useMemo(() => {
        const params = new URLSearchParams(searchParams.toString())
        stripFilters(params)
        return queryFromSearchParams(params)
    }, [searchParams, stripFilters])

    React.useEffect(() => {
        if (!profile.data) return
        const params = new URLSearchParams(searchParams.toString())
        if (!stripFilters(params)) return
        const qs = params.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }, [pathname, profile.data, router, searchParams, stripFilters])

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            patchSearchParams({ router, pathname, searchParams }, patch, {
                replace: true,
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const listQuery = useBooks(query)
    const [hasSettled, setHasSettled] = React.useState(!listQuery.isPending)
    React.useEffect(() => {
        if (!listQuery.isPending) setHasSettled(true)
    }, [listQuery.isPending])
    const rows = listQuery.data?.rows ?? EMPTY_BOOKS
    const total = listQuery.data?.total ?? 0
    const scopeConflict =
        listQuery.isError && isDataScopeChanged(listQuery.error)
    const statusView = query.status ?? "ALL"
    const hasQueryFilters = hasActiveQueryFilters(query)

    const handleApply = React.useCallback(
        (next: BookListQuery) => {
            patchUrl({
                q: next.q ?? null,
                selection_form:
                    next.selection_form && next.selection_form !== "ALL"
                        ? next.selection_form
                        : null,
                owner_user_ids: canFilterOwners
                    ? (next.owner_user_ids ?? null)
                    : null,
                org_unit_ids: canFilterOrgs
                    ? (next.org_unit_ids ?? null)
                    : null,
                include_descendants:
                    canFilterOrgs &&
                    next.org_unit_ids &&
                    next.include_descendants
                        ? "1"
                        : null,
                page: null,
            })
        },
        [canFilterOrgs, canFilterOwners, patchUrl],
    )

    const handleReset = React.useCallback(() => {
        patchUrl({
            q: null,
            selection_form: null,
            owner_user_ids: null,
            org_unit_ids: null,
            include_descendants: null,
            page: null,
        })
    }, [patchUrl])

    const handleEmptyReset = () => {
        if (hasQueryFilters) handleReset()
        else patchUrl({ status: null, page: null })
    }

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            patchUrl({
                page: next.pageIndex > 0 ? String(next.pageIndex + 1) : null,
                page_size:
                    next.pageSize !== DEFAULT_PAGE_SIZE
                        ? String(next.pageSize)
                        : null,
            })
        },
        [patchUrl],
    )

    const goToFirstPage = () => patchUrl({ page: null })
    const handleRetry = () => {
        if (scopeConflict && (query.page ?? 1) > 1) goToFirstPage()
        else void listQuery.refetch()
    }

    const handleRowNavigate = React.useCallback(
        (bookId: string) => {
            router.push(`/sales/selection/${bookId}`)
        },
        [router],
    )

    const handleCopy = React.useCallback((book: SelectionBook) => {
        lastFocusedRowId.current = bookIdentity(book)
        setCopySource(book)
    }, [])
    const closeCopy = () => {
        const rowId = lastFocusedRowId.current
        lastFocusedRowId.current = null
        setCopySource(null)
        if (rowId) {
            requestAnimationFrame(() => {
                const row = document
                    .getElementById("sales-selection-list-table")
                    ?.querySelector<HTMLElement>(
                        `[data-row-id="${CSS.escape(rowId)}"][tabindex]`,
                    )
                const focusTarget = row ?? searchInputRef.current
                focusTarget?.focus({ preventScroll: true })
            })
        }
    }

    return (
        <ListWorkspacePage
            title="选品册"
            loading={listQuery.isPending && !hasSettled}
            resultsHeadingId="sales-selection-list-results-heading"
            resultsLabel={
                listQuery.data && !listQuery.isError
                    ? `选品册列表，共 ${total} 条结果`
                    : undefined
            }
            views={{
                ariaLabel: "选品册状态",
                items: BOOK_STATUS_VIEWS.map(({ value, label }) => ({
                    id: `sales-selection-list-view-${toAutomationIdSegment(value)}`,
                    label,
                    count:
                        value === statusView &&
                        !listQuery.isError &&
                        listQuery.data
                            ? total
                            : undefined,
                    active: value === statusView,
                    onClick: () =>
                        patchUrl({
                            status: value === "ALL" ? null : value,
                            page: null,
                        }),
                })),
            }}
            actions={
                canStartFromPool
                    ? [
                          {
                              tier: "primary",
                              actionKey: "create",
                              id: "sales-selection-list-create",
                              label: "从商品池发起",
                              icon: PackageSearchIcon,
                              render: <Link href={POOL_HREF} />,
                          },
                      ]
                    : undefined
            }
            surface={{
                ariaLabel: "选品册列表",
                toolbar: (
                    <BooksListFilterBar
                        query={query}
                        searchInputRef={searchInputRef}
                        canFilterOwners={canFilterOwners}
                        canFilterOrgs={canFilterOrgs}
                        onApply={handleApply}
                        onReset={handleReset}
                        loading={listQuery.isPending || listQuery.isFetching}
                        failed={listQuery.isError}
                    />
                ),
                tableClassName: booksListStyles.table,
                table: (
                    <BooksListTable
                        rows={rows}
                        total={total}
                        loading={listQuery.isPending || listQuery.isFetching}
                        failed={listQuery.isError}
                        error={listQuery.error}
                        scopeConflict={scopeConflict}
                        onRetry={handleRetry}
                        filtersActive={hasQueryFilters || statusView !== "ALL"}
                        noScope={listQuery.data?.noScope ?? false}
                        onClearFilters={handleEmptyReset}
                        clearFiltersLabel={
                            hasQueryFilters ? undefined : "查看全部"
                        }
                        page={query.page ?? 1}
                        pageSize={query.page_size ?? DEFAULT_PAGE_SIZE}
                        onPaginationChange={handlePaginationChange}
                        onFirstPage={goToFirstPage}
                        canOpen={canOpen}
                        canOpenProposal={canOpenProposal}
                        createHref={canStartFromPool ? POOL_HREF : undefined}
                        onRowNavigate={handleRowNavigate}
                        onCopy={canCopy ? handleCopy : undefined}
                    />
                ),
            }}
        >
            {canCopy && copySource ? (
                <BookCopyDialog
                    source={copySource}
                    onClose={closeCopy}
                    onCopied={(detail) => {
                        lastFocusedRowId.current = null
                        setCopySource(null)
                        router.push(`/sales/selection/${bookIdentity(detail)}`)
                    }}
                />
            ) : null}
        </ListWorkspacePage>
    )
}
