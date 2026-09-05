"use client"

import { DownloadIcon, ShieldCheckIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    AccessListToolbar,
    AuditAdvancedFilterPanel,
} from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessViewTable } from "@/features/access-audit/pages/components/access-view-table"
import type { AuditEventRow } from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

/**
 * 审计查询：追加式事件的只读查询页。
 *
 * 与权限配置分开：查询词、筛选维度、时间语义与导出策略都不同，
 * 进入时默认落最近 7 天，不再以空列表迎客。
 */
export function AuditPage() {
    const page = useAccessAuditPage("audit")

    if (page.pageQuery.isPending) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-10 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    const data = page.data
    const rows = data?.auditEvents ?? []
    const auditPolicy = data?.governancePolicies.auditAccessPolicy
    const coverageHint =
        data?.auditCoverageFrom && data.auditCoverageTo
            ? `覆盖 ${formatDateTime(data.auditCoverageFrom, "full")} ~ ${formatDateTime(data.auditCoverageTo, "full")}`
            : "选择事件查看详情"

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="审计查询"
                description="按时间、操作者与对象查询审计事件。"
            >
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="operations-audit-go-access-config"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => page.routerPush("/system/access-audit")}
                    >
                        <ShieldCheckIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        权限配置
                    </Button>
                    <Button
                        id="operations-audit-export"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={page.exportBlocked}
                        title={
                            page.exportBlocked
                                ? (page.exportBlocker?.message ?? "导出已禁用")
                                : "导出当前筛选的审计事件"
                        }
                        onClick={() => page.handleExport()}
                    >
                        <DownloadIcon className="size-3.5" aria-hidden="true" />
                        导出审计
                    </Button>
                </div>
            </ListWorkspaceHeader>

            {auditPolicy?.state === "MISSING" ? (
                <Alert variant="info">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>审计查询窗口受限</AlertTitle>
                    <AlertDescription>
                        审计访问策略尚未配置，当前只提供保守窗口内的查询，导出已禁用。
                    </AlertDescription>
                </Alert>
            ) : null}

            {page.actionError ? (
                <Alert variant="destructive">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>操作提示</AlertTitle>
                    <AlertDescription>{page.actionError}</AlertDescription>
                </Alert>
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

            <ListWorkSurface
                ariaLabel="审计事件列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="审计查询视图"
                        hint={coverageHint}
                        items={[
                            {
                                id: "operations-audit-view-events",
                                label: "审计事件",
                                count: rows.length,
                                active: true,
                                onClick: () => page.switchView("audit"),
                            },
                        ]}
                    />
                }
                toolbar={
                    <>
                        <AccessListToolbar
                            isAudit
                            searchInputRef={page.searchInputRef}
                            searchDraft={page.searchDraft}
                            setSearchDraft={page.setSearchDraft}
                            panelOpen={page.panelOpen}
                            setPanelOpen={page.setPanelOpen}
                            hasStructuredFilters={page.hasStructuredFilters}
                            appliedChips={page.appliedChips}
                            hasChips={
                                page.hasActiveFilters &&
                                page.appliedChips.length > 0
                            }
                            removeFilter={page.removeFilter}
                            clearAllFilters={page.clearFilters}
                            applyFilters={page.applyFilters}
                        />
                        {page.panelOpen ? (
                            <AuditAdvancedFilterPanel
                                draft={page.draft}
                                updateDraft={page.updateDraft}
                                actionOptions={page.actionOptions}
                                filterError={page.filterError}
                                resetMoreFilters={page.resetMoreFilters}
                                applyFilters={page.applyFilters}
                            />
                        ) : null}
                    </>
                }
                table={
                    <AccessViewTable
                        view="audit"
                        isAudit
                        rows={rows}
                        pagination={page.pagination}
                        onPaginationChange={page.handlePaginationChange}
                        isFetching={
                            page.pageQuery.isFetching &&
                            !page.pageQuery.isPending
                        }
                        emptyReason={data?.emptyReason}
                        roleColumns={page.roleColumns}
                        userColumns={page.userColumns}
                        auditColumns={page.auditColumns}
                        onClearFilters={page.clearFilters}
                        onRowPreview={(row) =>
                            page.openEvent((row as AuditEventRow).auditEventId)
                        }
                        errorState={
                            page.pageQuery.isError && !data ? (
                                <BusinessFailureState
                                    error={page.pageQuery.error}
                                    action={
                                        <Button
                                            id="operations-audit-retry"
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={() =>
                                                void page.pageQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />

            <AccessPreviewSheets
                explainSubject={page.explainSubject}
                eventOpenId={page.eventOpenId}
                effectiveQuery={page.effectiveQuery}
                eventQuery={page.eventQuery}
                closeExplain={page.closeExplain}
                closeEvent={page.closeEvent}
                restoreRowFocus={page.restoreRowFocus}
            />
        </PageScaffold>
    )
}
