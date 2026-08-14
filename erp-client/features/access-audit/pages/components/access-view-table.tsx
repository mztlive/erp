"use client"

import type { ReactNode } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { BusinessTableFrame, DataTable } from "@/components/business"
import { EmptyByReason } from "@/features/access-audit/components/empty-by-reason"
import { ACCESS_VIEW_LABEL } from "@/features/access-audit/types"
import type {
    AccessEmptyReason,
    AccessView,
    AuditEventRow,
    FieldPolicyRow,
    RoleRow,
    ScopeRow,
    UserRow,
} from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

type ViewRows =
    | readonly RoleRow[]
    | readonly UserRow[]
    | readonly ScopeRow[]
    | readonly FieldPolicyRow[]
    | readonly AuditEventRow[]

type AccessViewTableProps = {
    view: AccessView
    isAudit: boolean
    rows: ViewRows
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    isFetching: boolean
    emptyReason?: AccessEmptyReason
    auditCoverageFrom?: string
    auditCoverageTo?: string
    roleColumns: ColumnDef<RoleRow>[]
    userColumns: ColumnDef<UserRow>[]
    scopeColumns: ColumnDef<ScopeRow>[]
    fieldColumns: ColumnDef<FieldPolicyRow>[]
    auditColumns: ColumnDef<AuditEventRow>[]
    onClearFilters?: () => void
    toolbar?: ReactNode
}

function AccessViewTable({
    view,
    isAudit,
    rows,
    pagination,
    onPaginationChange,
    isFetching,
    emptyReason,
    auditCoverageFrom,
    auditCoverageTo,
    roleColumns,
    userColumns,
    scopeColumns,
    fieldColumns,
    auditColumns,
    onClearFilters,
    toolbar,
}: AccessViewTableProps) {
    const pagedRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )

    return (
        <BusinessTableFrame
            title={ACCESS_VIEW_LABEL[view]}
            description={
                emptyReason && emptyReason !== "FIELD_MASKED"
                    ? "当前无列表数据，可调整筛选后重试"
                    : isAudit && auditCoverageFrom && auditCoverageTo
                      ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(auditCoverageFrom, "full")} ~ ${formatDateTime(auditCoverageTo, "full")} · 无记录不等于动作未发生`
                      : `共 ${rows.length} 条`
            }
            toolbar={toolbar}
            table={
                emptyReason && emptyReason !== "FIELD_MASKED" ? (
                    <EmptyByReason
                        reason={emptyReason}
                        onClearFilters={
                            emptyReason === "FILTER_NO_RESULT"
                                ? onClearFilters
                                : undefined
                        }
                    />
                ) : view === "roles" ? (
                    <DataTable
                        columns={roleColumns}
                        data={pagedRows as RoleRow[]}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                        density="compact"
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                    />
                ) : view === "users" ? (
                    <DataTable
                        columns={userColumns}
                        data={pagedRows as UserRow[]}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                        density="compact"
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                    />
                ) : view === "scopes" ? (
                    <DataTable
                        columns={scopeColumns}
                        data={pagedRows as ScopeRow[]}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                        density="compact"
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["subject"],
                            right: ["actions"],
                        }}
                    />
                ) : view === "fields" ? (
                    <DataTable
                        columns={fieldColumns}
                        data={pagedRows as FieldPolicyRow[]}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                        density="compact"
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["target"],
                            right: ["actions"],
                        }}
                    />
                ) : (
                    <DataTable
                        columns={auditColumns}
                        data={pagedRows as AuditEventRow[]}
                        getRowId={(row) => row.auditEventId}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                        density="compact"
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["time"],
                            right: ["actions"],
                        }}
                    />
                )
            }
        />
    )
}

export { AccessViewTable }
