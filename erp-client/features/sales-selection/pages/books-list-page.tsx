/**
 * 选品册列表：工作区页头 + 状态视图 + 查询条 + 表格。
 */

"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
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
import { booksListStyles as listStyles } from "./books-list-styles"

const DEFAULT_PAGE_SIZE = 20

function emptyQuery(): BookListQuery {
    return { page: 1, page_size: DEFAULT_PAGE_SIZE }
}

function filtersActive(query: BookListQuery): boolean {
    return Boolean(
        query.q ||
        (query.selection_form && query.selection_form !== "ALL") ||
        (query.submit_mode && query.submit_mode !== "ALL") ||
        (query.status && query.status !== "ALL"),
    )
}

/**
 * 选品册列表页。
 */
export const BooksListPage = () => {
    const router = useRouter()
    const [query, setQuery] = React.useState<BookListQuery>(emptyQuery)
    const listQuery = useBooks(query)
    const rows = listQuery.data?.rows ?? []
    const total = listQuery.data?.total ?? 0
    const queriedAt = listQuery.data
        ? new Date(listQuery.dataUpdatedAt).toISOString()
        : undefined
    const statusView = query.status ?? "ALL"

    const handleApply = React.useCallback((next: BookListQuery) => {
        setQuery({ ...next, page: 1 })
    }, [])

    const handleReset = React.useCallback(() => {
        setQuery((prev) => ({
            ...emptyQuery(),
            status: prev.status,
            page_size: prev.page_size ?? DEFAULT_PAGE_SIZE,
        }))
    }, [])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setQuery((prev) => ({
                ...prev,
                page: next.pageIndex + 1,
                page_size: next.pageSize,
            }))
        },
        [],
    )

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
                                "查询失败"
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
                                    setQuery((prev) => ({
                                        ...prev,
                                        status:
                                            item.value === "ALL"
                                                ? "ALL"
                                                : item.value,
                                        page: 1,
                                    }))
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
                        onRetry={() => void listQuery.refetch()}
                        filtersActive={filtersActive(query)}
                        onClearFilters={() => setQuery(emptyQuery())}
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
