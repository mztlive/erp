"use client"

import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { Button } from "@/components/ui/button"
import { AccessListToolbar } from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessChangeDialog } from "@/features/access-audit/pages/components/access-change-dialog"
import { AccessViewTable } from "@/features/access-audit/pages/components/access-view-table"
import { DeleteRoleDialog } from "@/features/admin/delete-role-dialog"
import { BuiltinRoleDialog } from "@/features/admin/components/roles/builtin-role-dialog"
import type { RoleRow } from "@/features/access-audit/types"

/**
 * 权限配置：角色权限。
 *
 * 列表与侧栏只展示角色操作权限；人员数据范围在人员资料维护。
 * 用户授权不在这里：账号的角色绑定在账号管理中维护。
 */
export function AccessAuditPage() {
    const profile = useAccountProfileQuery()
    const page = useAccessAuditPage("access")
    const permissions = profile.data?.permissions
    const canCreate =
        hasPermission(permissions, "role:create") &&
        hasPermission(permissions, "role:list")
    const canEdit = hasPermission(permissions, "role:update")
    const canDelete = hasPermission(permissions, "role:delete")

    if (profile.isError) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader eyebrow="系统" title="角色与权限" />
                <BusinessFailureState
                    error={profile.error}
                    id="operations-access-profile-retry"
                    onRetry={() => void profile.refetch()}
                />
            </PageScaffold>
        )
    }

    if (page.rejectedWorkItemId) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader
                    eyebrow="系统"
                    title="角色与权限"
                    description="管理角色的操作权限，查看关联人员。"
                />
                <FormalActionResult
                    status="blocked"
                    title="权限复核入口未开放"
                    description="权限变更复核功能暂未开放，当前无法在此页面直接确认。"
                    facts={[
                        {
                            label: "说明",
                            value: "需配置复核规则后才能执行此操作",
                        },
                    ]}
                    actions={
                        <Button
                            id="operations-access-config-blocked-back"
                            type="button"
                            variant="outline"
                            onClick={() =>
                                page.patchUrl(
                                    {
                                        workItemId: null,
                                        queueContextId: null,
                                    },
                                    { replace: true },
                                )
                            }
                        >
                            返回权限配置
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

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
    const view = page.view
    const rows = data?.roles ?? []

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="角色与权限"
                description="管理角色的操作权限，查看关联人员。"
            >
                <div className="flex flex-wrap items-center gap-2">
                    {canCreate ? <BuiltinRoleDialog /> : null}
                    <Button
                        id="operations-access-export"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={page.exportBlocked}
                        title={
                            page.exportBlocked
                                ? (page.exportBlocker?.message ?? "导出已禁用")
                                : "导出当前筛选的配置"
                        }
                        onClick={() => page.handleExport()}
                    >
                        <DownloadIcon className="size-3.5" aria-hidden="true" />
                        导出配置
                    </Button>
                    {canCreate && (
                        <Button
                            id="operations-access-config-create-role"
                            type="button"
                            size="sm"
                            onClick={() => page.routerPush("/system/roles/new")}
                        >
                            <PlusIcon className="size-3.5" aria-hidden="true" />
                            新建角色
                        </Button>
                    )}
                </div>
            </ListWorkspaceHeader>

            {data ? (
                <PolicyBanner policies={data.governancePolicies} view={view} />
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
                            id="operations-access-config-result-close"
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
                ariaLabel="角色权限列表"
                toolbar={
                    <AccessListToolbar
                        isAudit={false}
                        searchInputRef={page.searchInputRef}
                        searchDraft={page.searchDraft}
                        setSearchDraft={page.setSearchDraft}
                        panelOpen={page.panelOpen}
                        setPanelOpen={page.setPanelOpen}
                        appliedChips={page.appliedChips}
                        removeFilter={page.removeFilter}
                        clearAllFilters={page.clearFilters}
                        applyFilters={page.applyFilters}
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
                        view={view}
                        isAudit={false}
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
                        highlightedRowId={page.explainSubject?.id}
                        onRowPreview={(row) =>
                            page.openExplain("ROLE", (row as RoleRow).id)
                        }
                        errorState={
                            page.pageQuery.isError && !data ? (
                                <BusinessFailureState
                                    error={page.pageQuery.error}
                                    action={
                                        <Button
                                            id="operations-access-config-retry"
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
                canEditRole={canEdit}
                previewRole={
                    page.explainSubject?.type === "ROLE"
                        ? (rows.find(
                              (row) => row.id === page.explainSubject?.id,
                          ) ?? null)
                        : null
                }
                eventOpenId={page.eventOpenId}
                effectiveQuery={page.effectiveQuery}
                eventQuery={page.eventQuery}
                closeExplain={page.closeExplain}
                closeEvent={page.closeEvent}
                restoreRowFocus={page.restoreRowFocus}
            />

            {/* 影响预览 + 提交 */}
            <AccessChangeDialog
                open={page.changeOpen}
                onOpenChange={(open) => {
                    page.setChangeOpen(open)
                    if (!open) {
                        page.setPendingCommand(null)
                        page.setImpact(null)
                    }
                }}
                impact={page.impact}
                pendingCommand={page.pendingCommand}
                isSubmitting={page.submitMutation.isPending}
                form={page.form}
                onApplyOutcome={page.applyOutcome}
            />

            {canDelete && page.deletingRole ? (
                <DeleteRoleDialog
                    key={page.deletingRole.id}
                    role={page.deletingRole}
                    onOpenChange={(open) => {
                        if (!open) page.setDeletingRole(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
