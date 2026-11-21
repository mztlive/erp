"use client"

import * as React from "react"
import Link from "next/link"
import { PlusIcon } from "lucide-react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { BusinessFailureState } from "@/components/business"
import { ListDataTable } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useSalesOrdersListKeyboardNav } from "@/features/sales-orders/hooks/use-sales-orders-list-keyboard-nav"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { buildSalesOrdersListColumns } from "./sales-orders-list-columns"

export function SalesOrdersListTable(props: {
    noScope?: boolean
    items: SalesOrderListItem[]
    total: number
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    filtersActive: boolean
    onClearFilters: () => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    canCreate: boolean
    createHref: string
    canOpen: boolean
    canDownloadContract: boolean
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
        sorting,
        onSortingChange,
        pagination,
        onPaginationChange,
        canCreate,
        createHref,
        canOpen,
        canDownloadContract,
        onRowNavigate,
        downloadingContractId,
        downloadContract,
    } = props

    useSalesOrdersListKeyboardNav(canOpen)

    const columns = React.useMemo(
        () =>
            buildSalesOrdersListColumns({
                canOpen,
                canDownloadContract,
                downloadingContractId,
                downloadContract,
            }),
        [canDownloadContract, canOpen, downloadingContractId, downloadContract],
    )

    return (
        <ListDataTable
            id="sales-orders-list-table"
            entityName="销售单"
            caption="销售单列表"
            data={items}
            columns={columns}
            defaultColumnVisibility={{
                nature: false,
                contract: false,
                currentOwner: false,
                submittedAt: false,
            }}
            defaultColumnOrder={[
                "document",
                "status",
                "tracks",
                "amount",
                "owner",
                "nature",
                "contract",
                "currentOwner",
                "submittedAt",
                "actions",
            ]}
            defaultColumnPinning={{ left: ["document"], right: ["actions"] }}
            getRowId={(row) => row.id}
            rowLabel={(row) => `${row.documentNumber} · ${row.customerName}`}
            rowCount={total}
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            loading={loading}
            errorState={
                isError ? (
                    <BusinessFailureState
                        id="sales-orders-list-retry"
                        title="销售单列表加载失败"
                        error={error}
                        onRetry={onRetry}
                    />
                ) : undefined
            }
            noScope={props.noScope}
            hasActiveFilters={filtersActive}
            onClearFilters={onClearFilters}
            clearFiltersButtonId="sales-orders-list-empty-clear"
            canCreate={canCreate}
            emptyAction={
                canCreate ? (
                    <Button
                        id="sales-orders-list-empty-create"
                        type="button"
                        size="sm"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        render={<Link href={createHref} />}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新建销售单
                    </Button>
                ) : undefined
            }
            showPagination={!isError && (!loading || total > 0)}
            emptyCopy={{
                noScopeTitle: "暂无销售单数据范围",
                noScopeDescription: "请联系管理员配置可查看的业务范围。",
                emptyDescription: canCreate
                    ? "当前业务范围内还没有销售单，可新建第一张单。"
                    : "当前业务范围内还没有销售单。",
            }}
            scrollable
            onRowOpen={canOpen ? (row) => onRowNavigate(row.id) : undefined}
        />
    )
}
