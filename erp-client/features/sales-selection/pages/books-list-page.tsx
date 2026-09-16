/**
 * 选品册列表：工作区页头 + 状态视图 + 查询条 + 表格。
 * 筛选进入 URL 与 QueryKey；无范围 / 筛空 / 失败分别呈现。
 */

"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PackageSearchIcon } from "lucide-react"
import type { PaginationState } from "@tanstack/react-table"

import { PageActions, PageScaffold } from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { BooksListFilterBar } from "@/features/sales-selection/components/books-list-filter-bar"
import { BooksListTable } from "@/features/sales-selection/components/books-list-table"
import { useBooks } from "@/features/sales-selection/hooks/queries"
import { BOOK_STATUS_VIEWS } from "@/features/sales-selection/lib/presentation"
import type { BookListQuery } from "@/features/sales-selection/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { ApiError } from "@/lib/api"
import { booksListStyles as listStyles } from "./books-list-styles"

const DEFAULT_PAGE_SIZE = 20

function queryFromSearchParams(searchParams: URLSearchParams): BookListQuery {
    const selectionForm = searchParams.get("selection_form")
    const submitMode = searchParams.get("submit_mode")
    const status = searchParams.get("status")
    const page = Number(searchParams.get("page") ?? "1")
    const pageSize = Number(searchParams.get("page_size") ?? "")
    return {
        q: searchParams.get("q") || undefined,
        selection_form:
            selectionForm === "SINGLE_SKU" || selectionForm === "PACKAGE"
                ? selectionForm
                : "ALL",
        submit_mode:
            submitMode === "BY_QUANTITY" || submitMode === "MALL_REDEEM"
                ? submitMode
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

function filtersActive(query: BookListQuery): boolean {
    return Boolean(
        query.q ||
        (query.selection_form && query.selection_form !== "ALL") ||
        (query.submit_mode && query.submit_mode !== "ALL") ||
        (query.status && query.status !== "ALL") ||
        query.owner_user_ids ||
        query.org_unit_ids,
    )
}

/** 跨页范围版本变化：回第一页刷新，不拼接新旧权限结果。 */
function isScopeConflict(error: unknown): boolean {
    if (typeof error !== "object" || error === null) return false
    const apiError = error as Partial<ApiError>
    return (
        apiError.status === 409 &&
        typeof apiError.message === "string" &&
        apiError.message.includes("DATA_SCOPE_CHANGED")
    )
}

/**
 * 选品册列表页。
 */
export const BooksListPage = () => {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const query = React.useMemo(
        () =>
            queryFromSearchParams(new URLSearchParams(searchParams.toString())),
        [searchParams],
    )

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const listQuery = useBooks(query)
    const rows = listQuery.data?.rows ?? []
    const total = listQuery.data?.total ?? 0
    const ownerOptions = listQuery.data?.ownerOptions ?? []
    const noScope = listQuery.data?.noScope ?? false
    const scopeConflict = listQuery.isError && isScopeConflict(listQuery.error)
    const queriedAt = listQuery.data
        ? new Date(listQuery.dataUpdatedAt).toISOString()
        : undefined
    const statusView = query.status ?? "ALL"

    const handleApply = React.useCallback(
        (next: BookListQuery) => {
            patchUrl({
                q: next.q ?? null,
                selection_form:
                    next.selection_form && next.selection_form !== "ALL"
                        ? next.selection_form
                        : null,
                submit_mode:
                    next.submit_mode && next.submit_mode !== "ALL"
                        ? next.submit_mode
                        : null,
                owner_user_ids: next.owner_user_ids ?? null,
                org_unit_ids: next.org_unit_ids ?? null,
                include_descendants: next.include_descendants ? "1" : null,
                page: null,
            })
        },
        [patchUrl],
    )

    const handleReset = React.useCallback(() => {
        patchUrl({
            q: null,
            selection_form: null,
            submit_mode: null,
            owner_user_ids: null,
            org_unit_ids: null,
            include_descendants: null,
            page: null,
        })
    }, [patchUrl])

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

    const handleRetry = React.useCallback(() => {
        if (scopeConflict) patchUrl({ page: null })
        else void listQuery.refetch()
    }, [listQuery, patchUrl, scopeConflict])

    const handleRowNavigate = React.useCallback(
        (bookId: string) => {
            router.push(`/sales/selection/${bookId}`)
        },
        [router],
    )

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="销售"
                title="选品册"
                description={
                    <>
                        一本选品册只发给一家客户。已提交行可打开销售方案核对。
                        <span className="ml-3 text-xs" role="status">
                            {listQuery.isError ? (
                                scopeConflict ? (
                                    "数据范围已变化，请从第一页刷新"
                                ) : (
                                    "查询失败"
                                )
                            ) : listQuery.isFetching ? (
                                "正在更新…"
                            ) : queriedAt ? (
                                <time dateTime={queriedAt}>
                                    更新于{" "}
                                    {new Date(queriedAt).toLocaleTimeString(
                                        "zh-CN",
                                        {
                                            hour: "2-digit",
                                            minute: "2-digit",
                                        },
                                    )}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            >
                <PageActions
                    actions={[
                        {
                            actionKey: "create",
                            id: "sales-selection-list-create",
                            label: "从商品池发起",
                            icon: PackageSearchIcon,
                            render: <Link href="/master-data/sellable-items" />,
                        },
                    ]}
                />
            </ListWorkspaceHeader>

            <ListWorkSurface
                ariaLabel="选品册列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="选品册状态"
                        hint="选择选品册查看详情"
                        items={BOOK_STATUS_VIEWS.map((item) => {
                            const active = statusView === item.value
                            return {
                                id: `sales-selection-list-view-${toAutomationIdSegment(item.value)}`,
                                label: item.label,
                                count: active ? total : undefined,
                                active,
                                onClick: () => {
                                    patchUrl({
                                        status:
                                            item.value === "ALL"
                                                ? null
                                                : item.value,
                                        page: null,
                                    })
                                },
                            }
                        })}
                    />
                }
                toolbar={
                    <BooksListFilterBar
                        query={query}
                        onApply={handleApply}
                        onReset={handleReset}
                        resultCount={listQuery.data ? total : undefined}
                        loading={listQuery.isPending || listQuery.isFetching}
                        failed={listQuery.isError}
                        ownerOptions={ownerOptions}
                    />
                }
                tableClassName={listStyles.table}
                table={
                    <BooksListTable
                        rows={rows}
                        total={total}
                        loading={listQuery.isPending}
                        failed={listQuery.isError}
                        error={listQuery.error}
                        onRetry={handleRetry}
                        filtersActive={filtersActive(query)}
                        noScope={noScope}
                        ownerOptions={ownerOptions}
                        onClearFilters={handleReset}
                        page={query.page ?? 1}
                        pageSize={query.page_size ?? DEFAULT_PAGE_SIZE}
                        onPaginationChange={handlePaginationChange}
                        onRowNavigate={handleRowNavigate}
                    />
                }
            />
        </PageScaffold>
    )
}
