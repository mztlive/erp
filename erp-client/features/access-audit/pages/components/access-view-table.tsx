"use client"

import type { ReactNode } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { DataTable, surfacePanelClassName } from "@/components/business"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { EmptyByReason } from "@/features/access-audit/components/empty-by-reason"
import { parseView } from "@/features/access-audit/lib/url-state"
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
import { cn } from "@/lib/utils"

const ACCESS_VIEWS: AccessView[] = ["roles", "users", "scopes", "audit"]

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
    onViewChange: (view: AccessView) => void
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
    onViewChange,
}: AccessViewTableProps) {
    const pagedRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )
    const description =
        emptyReason && emptyReason !== "FIELD_MASKED"
            ? "当前无列表数据，可调整筛选后重试"
            : isAudit && auditCoverageFrom && auditCoverageTo
              ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(auditCoverageFrom, "full")} ~ ${formatDateTime(auditCoverageTo, "full")} · 无记录不等于动作未发生`
              : `共 ${rows.length} 条`

    return (
        <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
            <nav aria-label="权限与审计二级导航">
                <Tabs
                    value={view}
                    onValueChange={(next) => onViewChange(parseView(next))}
                >
                    <TabsList
                        variant="line"
                        className="h-auto w-full flex-wrap justify-start gap-1 rounded-none border-b border-grid bg-card px-3 py-1.5"
                    >
                        {ACCESS_VIEWS.map((item) => (
                            <TabsTrigger
                                key={item}
                                value={item}
                                className="flex-none"
                            >
                                {ACCESS_VIEW_LABEL[item]}
                            </TabsTrigger>
                        ))}
                        <span className="ml-auto py-0.5 text-xs text-muted-foreground">
                            {description}
                        </span>
                    </TabsList>
                </Tabs>
            </nav>
            {toolbar ? (
                <>
                    <div className="px-3 py-2.5">{toolbar}</div>
                    <Separator />
                </>
            ) : null}
            <div data-slot="business-table-frame-table">
                {emptyReason && emptyReason !== "FIELD_MASKED" ? (
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
                        loading={isFetching}
                        showRefreshingBanner={isFetching}
                        defaultColumnPinning={{
                            left: ["time"],
                            right: ["actions"],
                        }}
                    />
                )}
            </div>
        </div>
    )
}

export { AccessViewTable }
