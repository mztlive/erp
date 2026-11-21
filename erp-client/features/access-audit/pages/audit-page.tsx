"use client"

import { useRouter, useSearchParams } from "@/lib/navigation"
import { DownloadIcon, ShieldCheckIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
} from "@/components/business"
import {
    ListDataTable,
    ListWorkspacePage,
    listWorkspaceEmptyStateClassName,
    type ListWorkspaceViewsConfig,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import { AccessListToolbar } from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BusinessAuditPage } from "@/features/audit/pages/business-audit-page"

const TITLE = "审计查询"

/** 身份与业务记录分别使用各自的目录和查询权限。 */
export function AuditPage() {
    const account = useAccountProfileQuery()
    const router = useRouter()
    const searchParams = useSearchParams()
    const canBusiness = hasPermission(
        account.data?.permissions,
        "audit_log:list",
    )
    const canIdentity = hasPermission(
        account.data?.permissions,
        "audit_event:list",
    )
    const source =
        canBusiness &&
        (searchParams.get("source") === "business" || !canIdentity)
            ? "business"
            : "identity"
    const selectSource = (next: "business" | "identity") => {
        const params = new URLSearchParams(searchParams.toString())
        params.set("source", next)
        router.replace(`/system/audit?${params}`, { scroll: false })
    }
    const views: ListWorkspaceViewsConfig = {
        ariaLabel: "审计记录类型",
        items: [
            ...(canBusiness
                ? [
                      {
                          id: "operations-audit-source-business",
                          label: "业务操作",
                          active: source === "business",
                          onClick: () => selectSource("business"),
                      },
                  ]
                : []),
            ...(canIdentity
                ? [
                      {
                          id: "operations-audit-source-identity",
                          label: "权限变更",
                          active: source === "identity",
                          onClick: () => selectSource("identity"),
                      },
                  ]
                : []),
        ],
    }

    if (account.isPending) {
        return <ListWorkspacePage title={TITLE} loading />
    }
    if (account.isError || (!canBusiness && !canIdentity)) {
        return (
            <ListWorkspacePage title={TITLE}>
                {account.isError ? (
                    <BusinessFailureState
                        error={account.error}
                        action={
                            <Button
                                id="operations-audit-account-retry"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => void account.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : (
                    <BusinessEmptyState
                        kind="no-scope"
                        className={listWorkspaceEmptyStateClassName}
                        title="暂无审计查询权限"
                        description="请联系管理员配置业务操作或权限变更的查询权限。"
                    />
                )}
            </ListWorkspacePage>
        )
    }
    return source === "business" ? (
        <BusinessAuditPage views={views} />
    ) : (
        <IdentityAuditPage
            views={views}
            canReadRoles={hasPermission(account.data?.permissions, "role:list")}
        />
    )
}

/**
 * 审计查询：追加式事件的只读查询页。
 *
 * 与权限配置分开：查询词、筛选维度、时间语义与导出策略都不同，
 * 进入时默认落最近 7 天，不再以空列表迎客。
 */
function IdentityAuditPage({
    views,
    canReadRoles,
}: {
    views: ListWorkspaceViewsConfig
    canReadRoles: boolean
}) {
    const page = useAccessAuditPage("audit")

    const { data, pageQuery, pagination } = page
    const rows = data?.auditEvents ?? []
    const pageStart = pagination.pageIndex * pagination.pageSize
    const emptyReason = data?.emptyReason
    const isError = pageQuery.isError && !data
    const noModulePermission = emptyReason === "NO_MODULE_PERMISSION"

    return (
        <ListWorkspacePage
            title={TITLE}
            views={views}
            loading={pageQuery.isPending}
            actions={[
                {
                    id: "operations-audit-go-access-config",
                    actionKey: "access-config",
                    label: "权限配置",
                    icon: ShieldCheckIcon,
                    tier: "quiet",
                    type: "button",
                    hidden: !canReadRoles,
                    onClick: () => page.routerPush("/system/access-audit"),
                },
                {
                    id: "operations-audit-export",
                    actionKey: "export",
                    label: "导出审计",
                    icon: DownloadIcon,
                    tier: "quiet",
                    type: "button",
                    disabled: page.exportBlocked,
                    title: page.exportBlocked
                        ? (page.exportBlocker?.message ?? "导出已禁用")
                        : "导出当前筛选的审计事件",
                    onClick: page.handleExport,
                },
            ]}
            alerts={
                <>
                    {data ? (
                        <PolicyBanner
                            policies={data.governancePolicies}
                            view="audit"
                        />
                    ) : null}
                    {page.lastResult ? (
                        <FormalActionResult
                            status={
                                page.lastResult.status === "failed"
                                    ? "blocked"
                                    : page.lastResult.status
                            }
                            title={page.lastResult.title}
                            description={page.lastResult.description}
                            reference={page.lastResult.reference}
                            facts={page.lastResult.facts}
                            actions={
                                <Button
                                    id="operations-audit-result-close"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() => page.setLastResult(null)}
                                >
                                    关闭
                                </Button>
                            }
                        />
                    ) : null}
                </>
            }
            notices={
                data?.auditCoverageFrom && data.auditCoverageTo ? (
                    <p className="num text-xs text-muted-foreground">
                        覆盖 {formatDateTime(data.auditCoverageFrom, "full")} ~{" "}
                        {formatDateTime(data.auditCoverageTo, "full")}。
                    </p>
                ) : null
            }
            surface={{
                ariaLabel: "审计事件列表",
                tableClassName: "min-h-64",
                toolbar: (
                    <AccessListToolbar
                        isAudit
                        searchInputRef={page.searchInputRef}
                        searchDraft={page.searchDraft}
                        setSearchDraft={page.setSearchDraft}
                        panelOpen={page.panelOpen}
                        setPanelOpen={page.setPanelOpen}
                        appliedChips={page.appliedChips}
                        removeFilter={page.removeFilter}
                        clearAllFilters={page.clearFilters}
                        applyFilters={page.applyFilters}
                        draft={page.draft}
                        updateDraft={page.updateDraft}
                        actionOptions={page.actionOptions}
                        filterError={page.filterError}
                        resetMoreFilters={page.resetMoreFilters}
                        onCancelMoreFilters={page.cancelMoreFilters}
                        hasPendingChanges={page.hasPendingChanges}
                        loading={pageQuery.isFetching}
                        failed={pageQuery.isError}
                    />
                ),
                table: (
                    <ListDataTable
                        id="operations-audit-events-table"
                        entityName="权限变更记录"
                        scrollable
                        data={rows.slice(
                            pageStart,
                            pageStart + pagination.pageSize,
                        )}
                        columns={page.auditColumns}
                        getRowId={(row) => row.auditEventId}
                        rowCount={rows.length}
                        rowLabel={(row) =>
                            `${row.actionLabel} ${row.objectLabel}`
                        }
                        pagination={pagination}
                        onPaginationChange={page.handlePaginationChange}
                        loading={pageQuery.isFetching}
                        defaultColumnPinning={{
                            left: ["object"],
                            right: ["actions"],
                        }}
                        highlightedRowId={page.eventOpenId ?? undefined}
                        onRowPreview={(row) => page.openEvent(row.auditEventId)}
                        isError={isError}
                        error={pageQuery.error}
                        onRetry={() => void pageQuery.refetch()}
                        retryButtonId="operations-audit-retry"
                        noScope={
                            noModulePermission ||
                            emptyReason === "NO_DATA_SCOPE"
                        }
                        hasActiveFilters={page.hasActiveFilters}
                        onClearFilters={page.clearFilters}
                        clearFiltersButtonId={
                            emptyReason === "FILTER_NO_RESULT"
                                ? "operations-audit-filter-empty-clear"
                                : "operations-audit-empty-clear-filters"
                        }
                        emptyCopy={{
                            noScopeTitle: noModulePermission
                                ? "暂无访问权限"
                                : "暂无可查看的权限变更记录",
                            noScopeDescription: noModulePermission
                                ? "当前账号无权查询权限变更记录，请联系系统管理员开通。"
                                : undefined,
                            filterDescription:
                                "没有记录符合当前条件，可放宽时间范围或调整筛选条件。",
                            emptyTitle: "暂无操作记录",
                        }}
                    />
                ),
            }}
        >
            <AccessPreviewSheets
                explainSubject={page.explainSubject}
                eventOpenId={page.eventOpenId}
                effectiveQuery={page.effectiveQuery}
                eventQuery={page.eventQuery}
                closeExplain={page.closeExplain}
                closeEvent={page.closeEvent}
                restoreRowFocus={page.restoreRowFocus}
            />
        </ListWorkspacePage>
    )
}
