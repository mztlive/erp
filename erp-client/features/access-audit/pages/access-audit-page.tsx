"use client"

import { useRef } from "react"
import { DownloadIcon, PlusIcon } from "lucide-react"

import { BusinessFailureState, FormalActionResult } from "@/components/business"
import {
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import type { AccessFilterKey } from "@/features/access-audit/pages/hooks/use-access-list-filters"
import { useAccessAuditPage } from "@/features/access-audit/pages/hooks/use-access-audit-page"
import { AccessChangeDialog } from "@/features/access-audit/pages/components/access-change-dialog"
import { DeleteRoleDialog } from "@/features/admin/delete-role-dialog"
import { BuiltinRoleDialog } from "@/features/admin/components/roles/builtin-role-dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

const TITLE = "角色与权限"
const ROLE_TABLE_ID = "operations-access-roles-table"

/**
 * 权限配置：角色权限。
 *
 * 列表与侧栏只展示角色操作权限；人员数据范围在人员资料维护。
 * 用户授权不在这里：账号的角色绑定在账号管理中维护。
 */
export function AccessAuditPage() {
    const profile = useAccountProfileQuery()
    const page = useAccessAuditPage("access")
    const previewedRoleId = useRef<string | null>(null)
    const permissions = profile.data?.permissions
    const canCreate =
        hasPermission(permissions, "role:create") &&
        hasPermission(permissions, "role:list")
    const canEdit = hasPermission(permissions, "role:update")
    const canDelete = hasPermission(permissions, "role:delete")

    if (profile.isError) {
        return (
            <ListWorkspacePage title={TITLE}>
                <BusinessFailureState
                    error={profile.error}
                    id="operations-access-profile-retry"
                    onRetry={() => void profile.refetch()}
                />
            </ListWorkspacePage>
        )
    }

    if (page.rejectedWorkItemId) {
        return (
            <ListWorkspacePage
                title={TITLE}
                eyebrow="系统"
                description="管理角色的操作权限，查看关联人员。"
            >
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
            </ListWorkspacePage>
        )
    }

    if (page.pageQuery.isPending || page.view === "audit") {
        return <ListWorkspacePage title={TITLE} loading />
    }

    const { data, pageQuery, pagination } = page
    const rows = data?.roles ?? []
    const pageStart = pagination.pageIndex * pagination.pageSize
    const emptyReason = data?.emptyReason
    const noModulePermission = emptyReason === "NO_MODULE_PERMISSION"
    const listFailed = pageQuery.isError || !data
    const createRole = () => page.routerPush("/system/roles/new")

    return (
        <ListWorkspacePage
            title={TITLE}
            headerActions={canCreate ? <BuiltinRoleDialog /> : null}
            actions={[
                {
                    id: "operations-access-export",
                    actionKey: "export",
                    label: "导出配置",
                    icon: DownloadIcon,
                    tier: "quiet",
                    type: "button",
                    disabled: page.exportBlocked,
                    title: page.exportBlocked
                        ? (page.exportBlocker?.message ?? "导出已禁用")
                        : "导出当前筛选的配置",
                    onClick: page.handleExport,
                },
                {
                    id: "operations-access-config-create-role",
                    actionKey: "create",
                    label: "新建角色",
                    icon: PlusIcon,
                    tier: "primary",
                    type: "button",
                    hidden: !canCreate,
                    onClick: createRole,
                },
            ]}
            alerts={
                data ? (
                    <PolicyBanner
                        policies={data.governancePolicies}
                        view={page.view}
                    />
                ) : null
            }
            surface={{
                ariaLabel: "角色权限列表",
                tableClassName:
                    "max-sm:[&_[data-column-id=identity]]:w-table-column-default max-sm:[&_[data-column-id=identity]]:min-w-table-column-default-min",
                toolbar: (
                    <ListWorkspaceFilterBar
                        idPrefix="operations-access-toolbar-filter"
                        formAriaLabel="角色查询"
                        morePresentation="popover"
                        onSubmit={page.applyFilters}
                        search={
                            <ListSearchField
                                id="operations-access-toolbar-search"
                                searchInputRef={page.searchInputRef}
                                value={page.searchDraft}
                                onChange={page.setSearchDraft}
                                placeholder="角色名称"
                                aria-label="搜索角色"
                            />
                        }
                        queryButtonId="operations-access-toolbar-query"
                        chips={page.appliedChips}
                        onClearChip={(key) =>
                            page.removeFilter(key as AccessFilterKey)
                        }
                        onClearAll={page.clearFilters}
                        clearButtonId="operations-access-toolbar-clear-all"
                        hasPendingChanges={page.hasPendingChanges}
                        pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                        resultStatus={
                            pageQuery.isFetching
                                ? "查询中…"
                                : listFailed
                                  ? "查询未完成"
                                  : null
                        }
                    />
                ),
                table: (
                    <ListDataTable
                        id={ROLE_TABLE_ID}
                        caption="角色权限列表"
                        entityName="角色"
                        data={rows.slice(
                            pageStart,
                            pageStart + pagination.pageSize,
                        )}
                        rowCount={rows.length}
                        columns={page.roleColumns}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.name}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: canEdit || canDelete ? ["actions"] : [],
                        }}
                        pagination={pagination}
                        onPaginationChange={page.handlePaginationChange}
                        loading={pageQuery.isFetching}
                        isError={listFailed}
                        error={pageQuery.error}
                        retryButtonId="operations-access-config-retry"
                        onRetry={() => void pageQuery.refetch()}
                        noScope={
                            noModulePermission ||
                            emptyReason === "NO_DATA_SCOPE"
                        }
                        hasActiveFilters={
                            page.hasActiveFilters ||
                            emptyReason === "FILTER_NO_RESULT"
                        }
                        clearFiltersButtonId="operations-access-filter-empty-clear"
                        onClearFilters={page.clearFilters}
                        canCreate={canCreate}
                        createButtonId="operations-access-empty-create-role"
                        createLabel="新建角色"
                        onCreate={createRole}
                        emptyCopy={{
                            noScopeTitle: noModulePermission
                                ? "暂无访问权限"
                                : "暂无可管理的角色或人员",
                            noScopeDescription: noModulePermission
                                ? "当前账号无权查看「角色与权限」，请联系系统管理员开通。"
                                : "当前账号尚未分配可管理的部门或角色范围，请联系系统管理员配置。",
                            emptyTitle: "暂无角色",
                            emptyDescription: canCreate
                                ? "可新建角色并为其配置操作权限。"
                                : undefined,
                        }}
                        highlightedRowId={page.explainSubject?.id}
                        onRowPreview={(row) => {
                            previewedRoleId.current = row.id
                            page.openExplain("ROLE", row.id)
                        }}
                    />
                ),
            }}
            notices={
                page.lastResult ? (
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
                ) : null
            }
        >
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
                restoreRowFocus={() => {
                    const id = previewedRoleId.current
                    previewedRoleId.current = null
                    if (!id) {
                        page.restoreRowFocus()
                        return
                    }
                    window.requestAnimationFrame(() => {
                        document
                            .getElementById(
                                `${ROLE_TABLE_ID}-row-${toAutomationIdSegment(id)}`,
                            )
                            ?.focus()
                    })
                }}
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
        </ListWorkspacePage>
    )
}
