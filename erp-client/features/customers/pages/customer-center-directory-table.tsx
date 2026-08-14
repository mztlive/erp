"use client"

import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import { BusinessEmptyState, DataTable } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import { SCOPE_LABELS } from "@/features/customers/lib/filter-customers"
import type {
    CustomerDirectoryItem,
    CustomerScope,
} from "@/features/customers/types"

/**
 * 客户中心目录结果区 table 槽位：空态/数据表三态渲染。
 * 纯展示组件，由页面嵌入 BusinessTableFrame；行打开与筛选清除通过回调交由页面处理。
 */
export function CustomerCenterDirectoryTable({
    items,
    totalInScope,
    columns,
    scope,
    status,
    q,
    canCreate,
    hasActiveFilters,
    sorting,
    onSortingChange,
    pagination,
    onPaginationChange,
    onClearFilters,
    onCreate,
    onOpenRow,
}: {
    items: readonly CustomerDirectoryItem[]
    totalInScope: number
    columns: ColumnDef<CustomerDirectoryItem>[]
    scope: CustomerScope
    status: DirectoryStatus
    q: string
    canCreate: boolean
    hasActiveFilters: boolean
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    onClearFilters: () => void
    onCreate: () => void
    onOpenRow: (row: CustomerDirectoryItem) => void
}) {
    return items.length === 0 ? (
        totalInScope === 0 && !q.trim() && status === "active" ? (
            <BusinessEmptyState
                kind="no-data"
                title="当前范围尚无客户"
                description={`${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`}
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    canCreate ? (
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onCreate}
                        >
                            新建客户
                        </Button>
                    ) : null
                }
            />
        ) : (
            <BusinessEmptyState
                kind="filter"
                title="当前筛选无结果"
                description={`范围“${SCOPE_LABELS[scope]}”${status !== "active" ? ` · 状态 ${status}` : ""}${q ? ` · 关键词“${q}”` : ""} 下没有匹配客户。`}
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    hasActiveFilters ? (
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null
                }
            />
        )
    ) : (
        <DataTable
            data={[...items]}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={totalInScope}
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            pageSizeOptions={[20]}
            layout="flush"
            density="compact"
            rowLabel={(row) => row.shortName || row.legalName}
            defaultColumnPinning={{
                left: ["customer"],
            }}
            onRowPreview={onOpenRow}
            onRowOpen={onOpenRow}
        />
    )
}
