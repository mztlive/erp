"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    ConnectionListItem,
    ConnectionListView,
} from "@/features/supplier-api-connections/types"

/**
 * 连接列表表格区：空态、错误态与数据表互斥切换；筛选区常驻不卸载（§11）。
 * 筛选无结果与系统尚无数据用同一清除/新建入口，空态动作共用工具栏的清除函数。
 */
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
    onRowOpen: (connectionId: string) => void
    onClearFilters: () => void
    onCreate: () => void
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    hasFilters: boolean
}) {
    const empty = data?.emptyReason
    const filterEmpty =
        empty === "FILTER_NO_RESULT" ||
        (empty === "NO_CONNECTIONS" && hasFilters)
    if (filterEmpty) {
        return (
            <BusinessEmptyState
                kind="filter"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="当前筛选无结果"
                description="没有连接符合当前环境/状态/能力/健康条件，可清除筛选。"
                action={
                    <Button
                        id="supplier-api-connections-list-clear-filters"
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        )
    }
    if (empty === "NO_CONNECTIONS") {
        return (
            <BusinessEmptyState
                kind="no-data"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="尚未接入供应商连接"
                description="当前环境还没有连接身份。有权限时可新建连接。"
                action={
                    data?.hasModulePermission ? (
                        <Button
                            id="supplier-api-connections-list-empty-create"
                            type="button"
                            onClick={onCreate}
                        >
                            新建连接
                        </Button>
                    ) : null
                }
            />
        )
    }
    return (
        <DataTable
            id="supplier-api-connections-list-table"
            data={data?.items ?? []}
            columns={columns}
            getRowId={(row) => row.connectionId}
            rowCount={data?.total ?? 0}
            rowLabel={(row) => row.connectionCode}
            caption="API 供应商连接列表"
            layout="flush"
            enableColumnPinning
            defaultColumnVisibility={{ owners: false }}
            defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
            }}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            onRowOpen={(row) => onRowOpen(row.connectionId)}
            loading={loading}
            errorState={
                isError && !data ? (
                    <BusinessFailureState
                        title="连接列表加载失败"
                        error={error}
                        onRetry={onRetry}
                    />
                ) : undefined
            }
        />
    )
}
