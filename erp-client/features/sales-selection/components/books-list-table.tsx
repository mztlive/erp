"use client"

import * as React from "react"
import Link from "next/link"
import type { PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    ListDataTable,
    listWorkspaceEmptyStateClassName,
} from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import { buildBooksListColumns } from "@/features/sales-selection/components/books-list-columns"
import {
    bookIdentity,
    formatBookInstant,
} from "@/features/sales-selection/lib/presentation"
import type { SelectionBook } from "@/features/sales-selection/types"

const DEFAULT_ORDER = [
    "customer",
    "owner",
    "form",
    "status",
    "display",
    "proposal",
    "createdAt",
    "actions",
]
const DEFAULT_VISIBILITY = { createdAt: false }
const DEFAULT_PINNING = { left: ["customer"], right: ["actions"] }
const PAGE_SIZE_OPTIONS = [20, 50, 100]
const EMPTY_COPY = {
    noScopeTitle: "当前没有可查询的数据范围",
    noScopeDescription:
        "当前授权范围未包含可读取的选品册，请联系权限管理员核对范围。",
    filterDescription: "调整查询条件或切换状态后再试。",
}

export function BooksListTable({
    rows,
    total,
    loading,
    failed,
    error,
    scopeConflict,
    onRetry,
    filtersActive,
    onClearFilters,
    clearFiltersLabel,
    noScope,
    page,
    pageSize,
    onPaginationChange,
    onFirstPage,
    canOpen,
    canOpenProposal,
    createHref,
    onRowNavigate,
    onCopy,
}: {
    rows: SelectionBook[]
    total: number
    loading: boolean
    failed: boolean
    error: unknown
    scopeConflict: boolean
    onRetry: () => void
    filtersActive: boolean
    onClearFilters: () => void
    clearFiltersLabel?: string
    noScope: boolean
    page: number
    pageSize: number
    onPaginationChange: (next: PaginationState) => void
    onFirstPage: () => void
    canOpen: boolean
    canOpenProposal: boolean
    createHref?: string
    onRowNavigate: (bookId: string) => void
    onCopy?: (book: SelectionBook) => void
}) {
    const columns = React.useMemo(
        () => buildBooksListColumns({ canOpen, canOpenProposal, onCopy }),
        [canOpen, canOpenProposal, onCopy],
    )
    const errorState = failed ? (
        <BusinessFailureState
            id="sales-selection-list-retry"
            title={
                scopeConflict
                    ? "数据范围已变化，请从第一页刷新"
                    : "选品册列表加载失败"
            }
            error={error}
            onRetry={scopeConflict ? undefined : onRetry}
            action={
                scopeConflict ? (
                    <Button
                        id="sales-selection-list-retry-retry"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onRetry}
                    >
                        从第一页刷新
                    </Button>
                ) : undefined
            }
        />
    ) : undefined
    const emptyPage = !noScope && page > 1 && rows.length === 0 && total > 0

    return (
        <ListDataTable
            id="sales-selection-list-table"
            entityName="选品册"
            caption="选品册列表"
            data={rows}
            columns={columns}
            getRowId={bookIdentity}
            rowLabel={(row) =>
                `${row.customer_name}的选品册（${formatBookInstant(row.created_at)}）`
            }
            rowCount={total}
            pagination={{ pageIndex: page - 1, pageSize }}
            onPaginationChange={onPaginationChange}
            pageSizeOptions={
                PAGE_SIZE_OPTIONS.includes(pageSize)
                    ? PAGE_SIZE_OPTIONS
                    : [...PAGE_SIZE_OPTIONS, pageSize].sort((a, b) => a - b)
            }
            loading={loading}
            errorState={errorState}
            noScope={noScope}
            hasActiveFilters={filtersActive}
            onClearFilters={onClearFilters}
            clearFiltersButtonId="sales-selection-list-empty-clear"
            clearFiltersLabel={clearFiltersLabel}
            emptyCopy={
                createHref
                    ? {
                          ...EMPTY_COPY,
                          emptyDescription:
                              "从公司商品池筛选或勾选商品后发起选品，选品册会显示在这里。",
                      }
                    : EMPTY_COPY
            }
            emptyAction={
                createHref && !noScope && !filtersActive ? (
                    <Link
                        id="sales-selection-list-empty-create"
                        href={createHref}
                        className={buttonVariants({
                            variant: "secondary",
                            size: "sm",
                            className: "rounded-lg shadow-none",
                        })}
                    >
                        从商品池发起
                    </Link>
                ) : undefined
            }
            emptyState={
                emptyPage ? (
                    <BusinessEmptyState
                        kind="no-data"
                        className={listWorkspaceEmptyStateClassName}
                        title="该页没有选品册"
                        description="返回第一页查看当前结果。"
                        action={
                            <Button
                                id="sales-selection-list-empty-first-page"
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onFirstPage}
                            >
                                返回第一页
                            </Button>
                        }
                    />
                ) : undefined
            }
            defaultColumnOrder={DEFAULT_ORDER}
            defaultColumnVisibility={DEFAULT_VISIBILITY}
            defaultColumnPinning={DEFAULT_PINNING}
            onRowOpen={
                canOpen ? (row) => onRowNavigate(bookIdentity(row)) : undefined
            }
        />
    )
}
