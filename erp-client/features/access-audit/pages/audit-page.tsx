"use client"

import { ShieldCheckIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    DataFreshness,
    FormalActionResult,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { AccessListToolbar } from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessViewTable } from "@/features/access-audit/pages/components/access-view-table"
import type { AccessView, AuditEventRow } from "@/features/access-audit/types"

const AUDIT_VIEWS: AccessView[] = ["audit"]

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
            <PageScaffold density="compact">
                <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-10 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    const data = page.data
    const rows = data?.auditEvents ?? []
    const auditPolicy = data?.governancePolicies.auditAccessPolicy

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="审计查询"
                description="按时间、操作者与对象查询审计事件；无记录不等于动作未发生。"
                metadata={
                    <DataFreshness
                        label="审计更新时间"
                        state={page.pageQuery.isFetching ? "syncing" : "fresh"}
                        updatedAt={
                            data
                                ? formatDateTime(data.calculatedAt, "full")
                                : "—"
                        }
                        dateTime={data?.calculatedAt}
                    />
                }
                actions={
                    <Button
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
                }
            />

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

            <AccessViewTable
                view="audit"
                isAudit
                views={AUDIT_VIEWS}
                rows={rows}
                pagination={page.pagination}
                onPaginationChange={page.handlePaginationChange}
                isFetching={
                    page.pageQuery.isFetching && !page.pageQuery.isPending
                }
                emptyReason={data?.emptyReason}
                auditCoverageFrom={data?.auditCoverageFrom}
                auditCoverageTo={data?.auditCoverageTo}
                roleColumns={page.roleColumns}
                userColumns={page.userColumns}
                auditColumns={page.auditColumns}
                onClearFilters={page.clearFilters}
                toolbar={
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
                        resetMoreFilters={page.resetMoreFilters}
                        applyFilters={page.applyFilters}
                        filterError={page.filterError}
                        draft={page.draft}
                        updateDraft={page.updateDraft}
                        actionOptions={page.actionOptions}
                    />
                }
                onViewChange={() => {}}
                onRowPreview={(row) =>
                    page.openEvent((row as AuditEventRow).auditEventId)
                }
                errorState={
                    page.pageQuery.isError && !data ? (
                        <BusinessFailureState
                            error={page.pageQuery.error}
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => void page.pageQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : undefined
                }
                exportBlocked={page.exportBlocked}
                exportBlocker={page.exportBlocker}
                onExport={page.handleExport}
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
