"use client"

import * as React from "react"
import Link from "next/link"
import type { PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { buildBooksListColumns } from "@/features/sales-selection/components/books-list-columns"
import { bookIdentity } from "@/features/sales-selection/lib/presentation"
import type { SelectionBook } from "@/features/sales-selection/types"

/**
 * 选品册结果表：贴齐工作区表格框架，失败与空态走统一组件。
 */
export function BooksListTable({
    rows,
    total,
    loading,
    failed,
    error,
    onRetry,
    filtersActive,
    onClearFilters,
    noScope = false,
    page,
    pageSize,
    onPaginationChange,
    onRowNavigate,
}: {
    rows: readonly SelectionBook[]
    total: number
    loading: boolean
    failed: boolean
    error: unknown
    onRetry: () => void
    filtersActive: boolean
    onClearFilters: () => void
    /** 无数据范围：与筛空区分展示，不提示创建解决。 */
    noScope?: boolean
    page: number
    pageSize: number
    onPaginationChange: (next: PaginationState) => void
    onRowNavigate: (bookId: string) => void
}) {
    const columns = React.useMemo(() => buildBooksListColumns(), [])
    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, page - 1),
            pageSize,
        }),
        [page, pageSize],
    )

    const errorState = failed ? (
        <BusinessFailureState
            id="sales-selection-list-retry"
            title="选品册列表加载失败"
            error={error}
            onRetry={onRetry}
        />
    ) : undefined

    const emptyState =
        !loading && rows.length === 0 ? (
            <BusinessEmptyState
                kind={noScope || filtersActive ? "filter" : "no-data"}
                className={listWorkspaceEmptyStateClassName}
                title={
                    noScope
                        ? "当前没有可查询的数据范围"
                        : filtersActive
                          ? "当前筛选无结果"
                          : "还没有选品册"
                }
                description={
                    noScope
                        ? "账号具备查询权限，但当前授权范围未包含可读取的选品册，请联系权限管理员核对范围。"
                        : filtersActive
                          ? "换一个关键词或清除筛选后再试。"
                          : "从公司商品池筛选或勾选商品后发起选品，选品册会显示在这里。"
                }
                action={
                    noScope ? undefined : filtersActive ? (
                        <Button
                            id="sales-selection-list-empty-clear"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : (
                        <Button
                            id="sales-selection-list-empty-create"
                            type="button"
                            size="sm"
                            render={<Link href="/master-data/sellable-items" />}
                        >
                            从商品池发起
                        </Button>
                    )
                }
            />
        ) : undefined

    return (
        <DataTable
            id="sales-selection-list-table"
            data={[...rows]}
            columns={columns}
            getRowId={(row) => bookIdentity(row)}
            rowCount={total}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            loading={loading}
            errorState={errorState}
            emptyState={emptyState}
            layout="flush"
            defaultColumnPinning={{ left: ["customer"] }}
            onRowOpen={(row) => onRowNavigate(bookIdentity(row))}
        />
    )
}
