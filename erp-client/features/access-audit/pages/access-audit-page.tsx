"use client"

import { LockIcon, PlusIcon, TriangleAlertIcon } from "lucide-react"

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
import { AccountFormDialog } from "@/features/admin/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/delete-admin-dialog"
import { DeleteRoleDialog } from "@/features/admin/delete-role-dialog"

export function AccessAuditPage() {
    const page = useAccessAuditPage()

    if (page.rejectedWorkItemId) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title="权限与审计"
                    description="此工作面只处理权限对象配置、解释和审计查询。"
                />
                <FormalActionResult
                    status="blocked"
                    title="权限复核入口未开放"
                    description="权限复核任务与专用复核命令尚未注册，不能在权限与审计页面代为确认。"
                    facts={[
                        {
                            label: "阻断原因",
                            value: "REVIEW_POLICY_UNCONFIGURED",
                        },
                    ]}
                    actions={
                        <Button
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
                            返回权限与审计
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

    const rows =
        view === "roles"
            ? (data?.roles ?? [])
            : view === "users"
              ? (data?.users ?? [])
              : view === "scopes"
                ? (data?.scopes ?? [])
                : view === "fields"
                  ? (data?.fieldPolicies ?? [])
                  : (data?.auditEvents ?? [])

    const listToolbar = (
        <AccessListToolbar
            isAudit={page.isAudit}
            searchInputRef={page.searchInputRef}
            searchDraft={page.searchDraft}
            setSearchDraft={page.setSearchDraft}
            panelOpen={page.panelOpen}
            setPanelOpen={page.setPanelOpen}
            hasStructuredFilters={page.hasStructuredFilters}
            appliedChips={page.appliedChips}
            hasChips={page.hasActiveFilters && page.appliedChips.length > 0}
            removeFilter={page.removeFilter}
            clearAllFilters={page.clearFilters}
            resetMoreFilters={page.resetMoreFilters}
            applyFilters={page.applyFilters}
            filterError={page.filterError}
            draft={page.draft}
            updateDraft={page.updateDraft}
            orgOptions={page.orgOptions}
        />
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="权限与审计"
                description={
                    page.isAudit
                        ? "查询追加式审计事件；无记录不等于动作未发生。"
                        : "配置角色、用户授权与数据范围，并查看有效权限来源。"
                }
                metadata={
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        <DataFreshness
                            label={
                                page.isAudit
                                    ? "审计更新时间"
                                    : "权限配置更新时间"
                            }
                            state={
                                page.pageQuery.isFetching ? "syncing" : "fresh"
                            }
                            updatedAt={
                                data
                                    ? formatDateTime(
                                          data.calculatedAt,
                                          "full",
                                      )
                                    : "—"
                            }
                            dateTime={data?.calculatedAt}
                        />
                        {!page.isAudit && data ? (
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                配置版本{" "}
                                <span className="num">
                                    v{data.permissionVersion.split("-").at(-1)}
                                </span>
                            </span>
                        ) : null}
                    </div>
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        {view === "roles" ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    page.routerPush("/system/roles/new")
                                }
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建角色
                            </Button>
                        ) : null}
                        {view === "users" ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    page.setAccountForm({
                                        mode: "create",
                                        account: null,
                                    })
                                }
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建账号
                            </Button>
                        ) : null}
                    </div>
                }
            />

            {data ? (
                <PolicyBanner
                    policies={data.governancePolicies}
                    view={view}
                />
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

            {data?.fieldMaskNote ? (
                <Alert variant="info">
                    <LockIcon aria-hidden="true" />
                    <AlertTitle>字段打码</AlertTitle>
                    <AlertDescription>{data.fieldMaskNote}</AlertDescription>
                </Alert>
            ) : null}

            <AccessViewTable
                view={view}
                isAudit={page.isAudit}
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
                scopeColumns={page.scopeColumns}
                fieldColumns={page.fieldColumns}
                auditColumns={page.auditColumns}
                onClearFilters={page.clearFilters}
                toolbar={listToolbar}
                onViewChange={page.switchView}
                errorState={
                    page.pageQuery.isError && !data ? (
                        <BusinessFailureState
                            error={page.pageQuery.error}
                            action={
                                <Button
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
                onConfirm={page.confirmChange}
                onApplyOutcome={page.applyOutcome}
            />

            {/* 账号新建 / 编辑（字段少，弹窗承载；角色走整页表单） */}
            {page.accountForm ? (
                <AccountFormDialog
                    key={
                        page.accountForm.mode === "edit"
                            ? (page.accountForm.account?.id ?? "edit")
                            : "create"
                    }
                    mode={page.accountForm.mode}
                    account={page.accountForm.account}
                    roleOptions={page.assignableRolesQuery.data ?? []}
                    onOpenChange={(open) => {
                        if (!open) page.setAccountForm(null)
                    }}
                />
            ) : null}

            {page.deletingAccount ? (
                <DeleteAdminDialog
                    key={page.deletingAccount.id}
                    account={page.deletingAccount}
                    onOpenChange={(open) => {
                        if (!open) page.setDeletingAccount(null)
                    }}
                />
            ) : null}

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
