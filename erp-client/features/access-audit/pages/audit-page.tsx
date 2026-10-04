"use client"

import type { ReactNode } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import { DownloadIcon, ShieldCheckIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessEmptyState,
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
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import { AccessListToolbar } from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessViewTable } from "@/features/access-audit/pages/components/access-view-table"
import type { AuditEventRow } from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BusinessAuditPage } from "@/features/audit/pages/business-audit-page"

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
    const views = (
        <ListWorkspaceViews
            ariaLabel="审计记录类型"
            items={[
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
            ]}
        />
    )

    if (account.isPending) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }
    if (account.isError || (!canBusiness && !canIdentity)) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader eyebrow="系统" title="审计查询" />
                {account.isError ? (
                    <BusinessFailureState
                        error={account.error}
                        action={
                            <Button
                                id="operations-audit-account-retry"
                                onClick={() => void account.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : (
                    <BusinessEmptyState
                        kind="no-scope"
                        title="暂无审计查询权限"
                        description="请联系管理员配置业务操作或权限变更的查询权限。"
                    />
                )}
            </PageScaffold>
        )
    }
    return source === "business" ? (
        <BusinessAuditPage views={views} />
    ) : (
        <IdentityAuditPage views={views} />
    )
}

/**
 * 审计查询：追加式事件的只读查询页。
 *
 * 与权限配置分开：查询词、筛选维度、时间语义与导出策略都不同，
 * 进入时默认落最近 7 天，不再以空列表迎客。
 */
function IdentityAuditPage({ views }: { views: ReactNode }) {
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
    const coverage =
        data?.auditCoverageFrom && data.auditCoverageTo
            ? `覆盖 ${formatDateTime(data.auditCoverageFrom, "full")} ~ ${formatDateTime(data.auditCoverageTo, "full")}。`
            : null

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="审计查询"
                description={
                    coverage
                        ? `按时间、操作者与对象查询审计事件。${coverage}`
                        : "按时间、操作者与对象查询审计事件。"
                }
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

            {data ? (
                <PolicyBanner policies={data.governancePolicies} view="audit" />
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
                views={views}
                toolbar={
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
                        resultCount={rows.length}
                        loading={
                            page.pageQuery.isFetching &&
                            !page.pageQuery.isPending
                        }
                        failed={page.pageQuery.isError && !data}
                    />
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
