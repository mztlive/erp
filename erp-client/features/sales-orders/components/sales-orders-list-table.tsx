"use client"

import * as React from "react"
import Link from "next/link"
import { PlusIcon } from "lucide-react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useSalesOrdersListKeyboardNav } from "@/features/sales-orders/hooks/use-sales-orders-list-keyboard-nav"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { buildSalesOrdersListColumns } from "./sales-orders-list-columns"

export function SalesOrdersListTable(props: {
    items: SalesOrderListItem[]
    total: number
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    filtersActive: boolean
    onClearFilters: () => void
    url: SalesOrdersUrlState
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    onRowNavigate: (id: string) => void
    downloadingContractId: string | null
    downloadContract: (order: SalesOrderListItem) => void
}) {
    const {
        items,
        total,
        loading,
        isError,
        error,
        onRetry,
        filtersActive,
        onClearFilters,
        url,
        sorting,
        onSortingChange,
        pagination,
        onPaginationChange,
        onRowNavigate,
        downloadingContractId,
        downloadContract,
    } = props

    useSalesOrdersListKeyboardNav({
        items,
        url,
        onRowNavigate,
    })

    const columns = React.useMemo(
        () =>
            buildSalesOrdersListColumns({
                downloadingContractId,
                downloadContract,
            }),
        [downloadingContractId, downloadContract],
    )

    if (isError) {
        return (
            <BusinessFailureState
                title="销售单列表加载失败"
                error={error}
                onRetry={onRetry}
            />
        )
    }

    if (!loading && items.length === 0) {
        return (
            <BusinessEmptyState
                kind={filtersActive ? "filter" : "no-data"}
                title={filtersActive ? undefined : "还没有销售单"}
                description={
                    filtersActive
                        ? "换一个关键词或清除筛选后再试。"
                        : "当前业务范围内还没有销售单，可新建第一张单。"
                }
                action={
                    filtersActive ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            render={<Link href="/sales/orders?mode=create" />}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            新建销售单
                        </Button>
                    )
                }
            />
        )
    }

    return (
        <DataTable
            data={items}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={total}
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            loading={loading}
            layout="flush"
            defaultColumnPinning={{ left: ["document"] }}
            onRowOpen={(row) => onRowNavigate(row.id)}
        />
    )
}
