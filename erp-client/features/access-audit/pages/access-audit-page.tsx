"use client"

import {
    DownloadIcon,
    PlusIcon,
    TriangleAlertIcon,
    UsersIcon,
} from "lucide-react"

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
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessChangeDialog } from "@/features/access-audit/pages/components/access-change-dialog"
import { AccessViewTable } from "@/features/access-audit/pages/components/access-view-table"
import { DeleteRoleDialog } from "@/features/admin/delete-role-dialog"
import type { RoleRow } from "@/features/access-audit/types"

/**
 * 权限配置：角色权限。
 *
 * 数据范围不再是并列页签——它是角色的属性，列表给摘要、整行点击看来源。
 * 用户授权不在这里：账号的角色绑定在账号管理中维护。
 */
export function AccessAuditPage() {
    const page = useAccessAuditPage("access")

    if (page.rejectedWorkItemId) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title="权限配置"
                    description="这里只处理角色权限与有效权限解释；用户授权在账号管理中维护。"
                />
                <FormalActionResult
                    status="blocked"
                    title="权限复核入口未开放"
                    description="权限复核任务与专用复核命令尚未注册，不能在权限配置页面代为确认。"
                    facts={[
                        {
                            label: "阻断原因",
                            value: "REVIEW_POLICY_UNCONFIGURED",
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
            <PageScaffold density="compact">
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
        <PageScaffold density="compact">
            <PageHeader
                title="权限配置"
                description="角色决定能做什么；在账号管理中绑定角色后立即生效；点击任意一行查看有效权限来源。"
                metadata={
                    <DataFreshness
                        label="配置更新时间"
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
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            id="operations-access-config-create-role"
                            type="button"
                            size="sm"
                            onClick={() => page.routerPush("/system/roles/new")}
                        >
                            <PlusIcon className="size-3.5" aria-hidden="true" />
                            新建角色
                        </Button>
                        <Button
                            id="operations-access-config-manage-accounts"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => page.routerPush("/system/accounts")}
                        >
                            <UsersIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            账号管理
                        </Button>
                    </div>
                }
            />

            {data ? (
                <PolicyBanner policies={data.governancePolicies} view={view} />
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

            <AccessViewTable
                view={view}
                isAudit={false}
                rows={rows}
                pagination={page.pagination}
                onPaginationChange={page.handlePaginationChange}
                isFetching={
                    page.pageQuery.isFetching && !page.pageQuery.isPending
                }
                emptyReason={data?.emptyReason}
                roleColumns={page.roleColumns}
                userColumns={page.userColumns}
                auditColumns={page.auditColumns}
                onClearFilters={page.clearFilters}
                toolbar={
                    <AccessListToolbar
                        isAudit={false}
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
                }
                headerAction={
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
                }
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

            <AccessPreviewSheets
                explainSubject={page.explainSubject}
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

            {page.deletingRole ? (
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
