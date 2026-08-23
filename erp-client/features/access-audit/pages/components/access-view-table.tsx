"use client"

import type { ReactNode } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { DownloadIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
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
    RoleRow,
    UserRow,
} from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

type ViewRows = readonly RoleRow[] | readonly UserRow[] | readonly AuditEventRow[]

type AccessViewTableProps = {
    view: AccessView
    isAudit: boolean
    /** 顶部二级导航展示的视图；只有一个时不渲染导航。 */
    views: readonly AccessView[]
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
    toolbar?: ReactNode
    onViewChange: (view: AccessView) => void
    /** 查询失败且无可用缓存时，替换表格内容的失败态（筛选区保持常驻）。 */
    errorState?: ReactNode
    exportBlocked?: boolean
    exportBlocker?: { message: string }
    onExport?: () => void
    /** 整行点击打开的详情（有效权限 / 审计事件）。 */
    onRowPreview?: (row: RoleRow | UserRow | AuditEventRow) => void
}

function AccessViewTable({
    view,
    isAudit,
    views,
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
    onViewChange,
    errorState,
    exportBlocked,
    exportBlocker,
    onExport,
    onRowPreview,
}: AccessViewTableProps) {
    const pagedRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )
    const description =
        emptyReason && emptyReason !== "FIELD_MASKED"
            ? "当前无列表数据，可调整筛选后重试"
            : isAudit && auditCoverageFrom && auditCoverageTo
              ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(auditCoverageFrom, "full")} ~ ${formatDateTime(auditCoverageTo, "full")}`
              : `共 ${rows.length} 条`
    const commonTableProps = {
        pagination,
        onPaginationChange,
        layout: "flush" as const,
        loading: isFetching,
        showRefreshingBanner: isFetching,
        rowCount: rows.length,
    }

    return (
        <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
            {views.length > 1 ? (
                <nav aria-label="权限配置二级导航">
                    <Tabs
                        value={view}
                        onValueChange={(next) => onViewChange(parseView(next))}
                    >
                        <TabsList
                            variant="line"
                            className="h-auto w-full flex-wrap justify-start gap-1 rounded-none border-b border-grid bg-card px-3 py-1.5"
                        >
                            {views.map((item) => (
                                <TabsTrigger
                                    key={item}
                                    value={item}
                                    className="flex-none"
                                >
                                    {ACCESS_VIEW_LABEL[item]}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                    </Tabs>
                </nav>
            ) : null}
            {toolbar ? (
                <>
                    <div className="flex flex-wrap items-start gap-2 px-3 py-2.5">
                        <div className="min-w-[16rem] flex-1">{toolbar}</div>
                        <div className="flex shrink-0 items-center gap-2 pt-0.5">
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                {description}
                            </span>
                            {onExport ? (
                                <Button
                                    type="button"
                                    variant="outline"
                                    disabled={exportBlocked}
                                    onClick={onExport}
                                >
                                    <DownloadIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    {isAudit ? "导出审计" : "导出配置"}
                                </Button>
                            ) : null}
                        </div>
                    </div>
                    {exportBlocked && exportBlocker ? (
                        <p className="px-3 pb-2 text-xs text-muted-foreground">
                            导出已禁用：{exportBlocker.message}
                        </p>
                    ) : null}
                    <Separator />
                </>
            ) : null}
            <div data-slot="business-table-frame-table">
                {errorState ? (
                    errorState
                ) : emptyReason && emptyReason !== "FIELD_MASKED" ? (
                    <EmptyByReason
                        reason={emptyReason}
                        isAudit={isAudit}
                        onClearFilters={onClearFilters}
                    />
                ) : view === "roles" ? (
                    <DataTable
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
                )}
            </div>
        </div>
    )
}

export { AccessViewTable }
