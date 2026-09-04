"use client"

import type { ReactNode } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { BusinessTableFrame, DataTable } from "@/components/business"
import { EmptyByReason } from "@/features/access-audit/components/empty-by-reason"
import type {
    AccessEmptyReason,
    AccessView,
    AuditEventRow,
    RoleRow,
    UserRow,
} from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

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
    auditCoverageFrom?: string
    auditCoverageTo?: string
    roleColumns: ColumnDef<RoleRow>[]
    userColumns: ColumnDef<UserRow>[]
    auditColumns: ColumnDef<AuditEventRow>[]
    onClearFilters?: () => void
    /** 搜索与筛选：独立工具条卡片，不嵌入表格卡片。 */
    toolbar?: ReactNode
    /** 与整张表结果相关的标题区动作（如导出）。 */
    headerAction?: ReactNode
    /** 查询失败且无可用缓存时，替换表格内容的失败态（筛选区保持常驻）。 */
    errorState?: ReactNode
    /** 整行点击打开的详情（有效权限 / 审计事件）。 */
    onRowPreview?: (row: RoleRow | UserRow | AuditEventRow) => void
}

const TABLE_TITLE: Record<AccessView, string> = {
    roles: "角色列表",
    users: "用户授权",
    scopes: "数据范围",
    fields: "字段策略",
    audit: "审计事件",
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
    auditColumns,
    onClearFilters,
    toolbar,
    headerAction,
    errorState,
    onRowPreview,
}: AccessViewTableProps) {
    const pagedRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )
    const coverage =
        isAudit && auditCoverageFrom && auditCoverageTo
            ? `覆盖 ${formatDateTime(auditCoverageFrom, "full")} ~ ${formatDateTime(auditCoverageTo, "full")}`
            : undefined
    const commonTableProps = {
        pagination,
        onPaginationChange,
        layout: "flush" as const,
        loading: isFetching,
        showRefreshingBanner: isFetching,
        rowCount: rows.length,
    }

    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    {TABLE_TITLE[view]}
                    <span
                        className="font-normal text-muted-foreground"
                        aria-live="polite"
                    >
                        {rows.length} 条
                    </span>
                </span>
            }
            description={coverage}
            toolbar={toolbar}
            headerActions={headerAction}
            table={
                errorState ? (
                    errorState
                ) : emptyReason && emptyReason !== "FIELD_MASKED" ? (
                    <EmptyByReason
                        reason={emptyReason}
                        isAudit={isAudit}
                        onClearFilters={onClearFilters}
                    />
                ) : view === "roles" ? (
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
                ) : view === "users" ? (
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
                ) : (
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
        />
    )
}

export { AccessViewTable }
