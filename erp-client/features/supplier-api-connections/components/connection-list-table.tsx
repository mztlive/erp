"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { ListDataTable } from "@/components/business"
import type {
    ConnectionListItem,
    ConnectionListView,
} from "@/features/supplier-api-connections/types"
import { getErrorPresentation } from "@/lib/api/errors"

export function ConnectionListTable({
    data,
    columns,
    pagination,
    onPaginationChange,
    onRowOpen,
    onClearFilters,
    onCreate,
    loading,
    isError,
    error,
    onRetry,
    hasFilters,
}: {
    data: ConnectionListView | undefined
    columns: ColumnDef<ConnectionListItem>[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    onRowOpen?: (connectionId: string) => void
    onClearFilters: () => void
    onCreate?: () => void
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    hasFilters: boolean
}) {
    const noPermission =
        data?.emptyReason === "NO_PERMISSION" ||
        data?.hasModulePermission === false
    const noScope =
        noPermission ||
        data?.emptyReason === "NO_SCOPE" ||
        data?.hasDataScope === false

    return (
        <ListDataTable
            id="supplier-api-connections-list-table"
            entityName="供应商连接"
            data={data?.items ?? []}
            columns={columns}
            getRowId={(row) => row.connectionId}
            rowCount={data?.total ?? 0}
            rowLabel={(row) => `${row.supplier.name}（${row.connectionCode}）`}
            caption="供应商平台连接列表"
            defaultColumnVisibility={{ owners: false, nextStep: false }}
            defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
            }}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            onRowOpen={
                onRowOpen ? (row) => onRowOpen(row.connectionId) : undefined
            }
            loading={loading}
            isError={isError}
            error={error}
            retryButtonId="supplier-api-connections-list-retry"
            onRetry={
                !isError || getErrorPresentation(error).retryable
                    ? onRetry
                    : undefined
            }
            noScope={noScope}
            hasActiveFilters={
                hasFilters || data?.emptyReason === "FILTER_NO_RESULT"
            }
            onClearFilters={onClearFilters}
            clearFiltersButtonId="supplier-api-connections-list-clear-filters"
            canCreate={Boolean(onCreate)}
            onCreate={onCreate}
            createButtonId="supplier-api-connections-list-empty-create"
            createLabel="新建连接"
            emptyCopy={{
                noScopeDescription: noPermission
                    ? "当前账号无权查看供应商连接，请联系管理员。"
                    : undefined,
                emptyTitle: "尚未接入供应商连接",
                emptyDescription: onCreate
                    ? "点击「新建连接」接入供应商平台。"
                    : undefined,
            }}
        />
    )
}
