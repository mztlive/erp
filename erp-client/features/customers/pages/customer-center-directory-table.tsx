"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { BusinessFailureState, ListDataTable } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useCustomerDirectoryColumns } from "@/features/customers/hooks/use-directory-columns"
import { SCOPE_LABELS } from "@/features/customers/lib/filter-customers"
import type {
    CustomerDirectoryItem,
    CustomerDirectoryResult,
    CustomerScope,
} from "@/features/customers/types"

export function CustomerCenterDirectoryTable({
    data,
    scope,
    canCreate,
    canOpenDetail,
    hasActiveFilters,
    sorting,
    onSortingChange,
    page,
    onPaginationChange,
    loading,
    failed,
    error,
    onRetry,
    onClearFilters,
    onCreate,
    onOpenRow,
}: {
    data?: CustomerDirectoryResult
    scope: CustomerScope
    canCreate: boolean
    canOpenDetail: boolean
    hasActiveFilters: boolean
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    page: number
    onPaginationChange: (next: PaginationState) => void
    loading: boolean
    failed: boolean
    error: unknown
    onRetry: () => void
    onClearFilters: () => void
    onCreate: () => void
    onOpenRow: (row: CustomerDirectoryItem) => void
}) {
    const columns = useCustomerDirectoryColumns(canOpenDetail)
    const items = React.useMemo(() => [...(data?.items ?? [])], [data?.items])
    const openRow = canOpenDetail ? onOpenRow : undefined

    return (
        <ListDataTable
            id="customers-directory-table"
            caption="客户目录"
            entityName="客户"
            data={items}
            columns={columns}
            getRowId={(row) => row.id}
            rowLabel={(row) =>
                `${row.shortName || row.legalName}（${row.customerNo}）`
            }
            rowCount={failed ? 0 : (data?.totalInScope ?? 0)}
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={{ pageIndex: page - 1, pageSize: 20 }}
            onPaginationChange={onPaginationChange}
            pageSizeOptions={[20]}
            showPagination={!failed && Boolean(data?.totalInScope)}
            defaultColumnOrder={["customer", "owner", "status", "business"]}
            defaultColumnVisibility={{ business: false }}
            defaultColumnPinning={{ left: ["customer"] }}
            loading={loading}
            errorState={
                failed ? (
                    <BusinessFailureState
                        title="客户目录加载失败"
                        error={error}
                        action={
                            <Button
                                id="customers-directory-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={onRetry}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : undefined
            }
            noScope={data?.emptyReason === "no_scope"}
            hasActiveFilters={hasActiveFilters}
            onClearFilters={onClearFilters}
            clearFiltersButtonId="customers-directory-empty-clear-filters"
            canCreate={canCreate}
            onCreate={onCreate}
            createButtonId="customers-directory-empty-create"
            createLabel="新建客户"
            emptyCopy={{
                noScopeDescription: "当前没有客户查看范围，请联系管理员配置。",
                filterDescription: `“${SCOPE_LABELS[scope]}”下没有符合条件的客户，可清除筛选后重试。`,
                emptyTitle: "当前范围尚无客户",
                emptyDescription: `“${SCOPE_LABELS[scope]}”下还没有客户。`,
            }}
            onRowPreview={openRow}
            onRowOpen={openRow}
        />
    )
}
