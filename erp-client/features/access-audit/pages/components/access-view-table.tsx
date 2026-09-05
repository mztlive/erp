"use client"

import type { ReactNode } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { DataTable } from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { EmptyByReason } from "@/features/access-audit/components/empty-by-reason"
import type {
    AccessEmptyReason,
    AccessView,
    AuditEventRow,
    RoleRow,
    UserRow,
} from "@/features/access-audit/types"
import { cn } from "@/lib/utils"

type ViewRows =
    | readonly RoleRow[]
    | readonly UserRow[]
    | readonly AuditEventRow[]

type AccessViewTableProps = {
    view: AccessView
    isAudit: boolean
    rows: ViewRows
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    isFetching: boolean
    emptyReason?: AccessEmptyReason
    roleColumns: ColumnDef<RoleRow>[]
    userColumns: ColumnDef<UserRow>[]
    auditColumns: ColumnDef<AuditEventRow>[]
    onClearFilters?: () => void
    /** 查询失败且无可用缓存时，替换表格内容的失败态（筛选区保持常驻）。 */
    errorState?: ReactNode
    /** 整行点击打开的详情（有效权限 / 审计事件）。 */
    onRowPreview?: (row: RoleRow | UserRow | AuditEventRow) => void
}

function AccessViewTable({
    view,
    isAudit,
    rows,
    pagination,
    onPaginationChange,
    isFetching,
    emptyReason,
    roleColumns,
    userColumns,
    auditColumns,
    onClearFilters,
    errorState,
    onRowPreview,
}: AccessViewTableProps) {
    const pagedRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )
    const commonTableProps = {
        pagination,
        onPaginationChange,
        layout: "flush" as const,
        loading: isFetching,
        showRefreshingBanner: isFetching,
        rowCount: rows.length,
        errorState,
        emptyState:
            emptyReason && emptyReason !== "FIELD_MASKED" ? (
                <div
                    className={cn(
                        listWorkspaceEmptyStateClassName,
                        "[&>[data-slot=business-empty-state]]:p-0",
                    )}
                >
                    <EmptyByReason
                        reason={emptyReason}
                        isAudit={isAudit}
                        onClearFilters={onClearFilters}
                    />
                </div>
            ) : undefined,
    }

    if (view === "roles") {
        return (
            <DataTable
                id="operations-access-roles-table"
                {...commonTableProps}
                columns={roleColumns}
                data={pagedRows as RoleRow[]}
                getRowId={(row) => row.id}
                onRowPreview={onRowPreview}
                defaultColumnPinning={{
                    left: ["identity"],
                    right: ["actions"],
                }}
            />
        )
    }

    if (view === "users") {
        return (
            <DataTable
                id="operations-access-users-table"
                {...commonTableProps}
                columns={userColumns}
                data={pagedRows as UserRow[]}
                getRowId={(row) => row.id}
                onRowPreview={onRowPreview}
                defaultColumnPinning={{
                    left: ["identity"],
                    right: ["actions"],
                }}
            />
        )
    }

    return (
        <DataTable
            id="operations-audit-events-table"
            {...commonTableProps}
            columns={auditColumns}
            data={pagedRows as AuditEventRow[]}
            getRowId={(row) => row.auditEventId}
            onRowPreview={onRowPreview}
            defaultColumnPinning={{
                left: ["time"],
                right: ["actions"],
            }}
        />
    )
}

export { AccessViewTable }
