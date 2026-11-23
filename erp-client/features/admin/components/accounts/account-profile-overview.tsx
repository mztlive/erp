"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronRightIcon,
    ShieldAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { OrganizationStateView } from "@/features/organization/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { resourceLabel } from "@/lib/permission-catalog"
import type { useRolesQuery } from "../../hooks/queries"
import type { usePersonScopes } from "../../hooks/use-person-data-scopes"
import { summarizePersonScopeViews } from "../../lib/account-detail-view"
import { adminAccountStatusBadge } from "../../lib/account-status"
import { roleCapabilitySummary } from "../../lib/role-summary"
import type { AdminAccount } from "../../types"

const OVERVIEW_SCOPE_LIMIT = 8

export function AccountProfileOverview({
    account,
    roles,
    orgData,
    scopes,
    canEdit,
    editDisabledReason,
    canReadRoles,
    canEditRoles,
    canReadScopes,
    onEdit,
    onOpenPermissions,
    onOpenScopes,
}: {
    account: AdminAccount
    roles: ReturnType<typeof useRolesQuery>
    orgData: OrganizationStateView | undefined
    scopes: ReturnType<typeof usePersonScopes>
    canEdit: boolean
    editDisabledReason?: string
    canReadRoles: boolean
    canEditRoles: boolean
    canReadScopes: boolean
    onEdit: () => void
    onOpenPermissions: () => void
    onOpenScopes: (resource?: string) => void
}) {
    const roleCount = account.role_ids.length

    const scopeSummary = React.useMemo(() => {
        if (!canReadScopes || !scopes.data) return null
        const labels = new Map(
            (orgData?.units ?? []).map((unit) => [unit.id, unit.name]),
        )
        return summarizePersonScopeViews(scopes.data, labels)
    }, [canReadScopes, orgData?.units, scopes.data])

    const prioritizedScopes = React.useMemo(() => {
        if (!scopeSummary) return []
        return [...scopeSummary.configurable].sort(
            (left, right) =>
                left.posture.priority - right.posture.priority ||
                resourceLabel(left.business.resource).localeCompare(
                    resourceLabel(right.business.resource),
                    "zh-CN",
                ),
        )
    }, [scopeSummary])

    const visibleScopes = prioritizedScopes.slice(0, OVERVIEW_SCOPE_LIMIT)

    // 仅在无权读取角色和数据范围时展示账号摘要，避免与页头重复或出现空主列。
    if (!canReadRoles && !canReadScopes) {
        return (
            <DetailWorkspaceSection
                title="账号资料"
                headingId="account-profile-title"
                width="full"
            >
                <DetailWorkspaceFacts columns={3} ariaLabel="账号基本信息">
                    <DetailWorkspaceFact
                        label="人员姓名"
                        value={account.name}
                    />
                    <DetailWorkspaceFact
                        label="登录账号"
                        numeric
                        value={account.account}
                    />
                    <DetailWorkspaceFact
                        label="账号状态"
                        value={
                            <StatusBadge
                                {...adminAccountStatusBadge(account.status)}
                            />
                        }
                    />
                    <DetailWorkspaceFact
                        label="创建时间"
                        numeric
                        value={formatDateTime(
                            new Date(account.created_at * 1000).toISOString(),
                            "full",
                        )}
                    />
                </DetailWorkspaceFacts>
            </DetailWorkspaceSection>
        )
    }

    return (
        <>
            {canReadRoles ? (
                <DetailWorkspaceSection
                    id="account-profile-roles-section"
                    title="角色分配"
                    headingId="account-profile-roles-title"
                    width="full"
                    badge={
                        roleCount > 0 ? (
                            <Badge variant="secondary" className="num">
                                {roleCount} 个
                            </Badge>
                        ) : null
                    }
                    actions={
                        roleCount > 0 ? (
                            <Button
                                id="account-profile-open-permissions"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                onClick={onOpenPermissions}
                            >
                                查看合并权限
                                <ChevronRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null
                    }
                >
                    {roles.isError ? (
                        <BusinessFailureState
                            id="account-detail-roles-retry"
                            error={roles.error}
                            onRetry={() => void roles.refetch()}
                        />
                    ) : roles.isPending && !roles.data ? (
                        <div className="space-y-3" aria-busy="true">
                            <div className="h-12 animate-pulse rounded-md bg-muted/70" />
                            <div className="h-12 animate-pulse rounded-md bg-muted/50" />
                        </div>
                    ) : !roleCount ? (
                        <div className="flex flex-wrap items-center justify-between gap-4 border-y border-border/60 py-5">
                            <div className="flex min-w-0 items-start gap-3">
                                <ShieldAlertIcon
                                    className="mt-0.5 size-4 shrink-0 text-warning"
                                    aria-hidden="true"
                                />
                                <div className="min-w-0 space-y-1">
                                    <p className="font-medium text-foreground">
                                        尚未分配角色
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        没有角色的账号无法执行任何业务操作。
                                    </p>
                                </div>
                            </div>
                            {canEdit ? (
                                <Button
                                    id="account-profile-assign-role"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={Boolean(editDisabledReason)}
                                    title={editDisabledReason}
                                    onClick={onEdit}
                                >
                                    分配角色
                                </Button>
                            ) : null}
                        </div>
                    ) : (
                        <ul className="divide-y divide-border/60 border-y border-border/60">
                            {account.role_ids.map((roleId) => {
                                const role = roles.data?.find(
                                    (row) => row.id === roleId,
                                )
                                if (!role) {
                                    return (
                                        <li
                                            key={roleId}
                                            className="flex items-center justify-between gap-3 py-3 text-xs text-muted-foreground"
                                        >
                                            <span>角色信息待确认</span>
                                        </li>
                                    )
                                }
                                const hasAllPermissions =
                                    role.permissions.includes("*:*") ||
                                    role.permissions.includes("*")
                                const permissionCountLabel = hasAllPermissions
                                    ? "全部权限"
                                    : `${new Set(role.permissions).size} 项操作`
                                return (
                                    <li
                                        key={roleId}
                                        className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-3"
                                    >
                                        <div className="min-w-0 flex-1 space-y-0.5">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium text-foreground">
                                                    {role.name}
                                                </span>
                                                {role.system ? (
                                                    <Badge variant="indigo">
                                                        系统内置
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            <p className="text-xs text-muted-foreground">
                                                {roleCapabilitySummary(
                                                    role.permissions,
                                                )}
                                            </p>
                                        </div>
                                        <div className="flex shrink-0 items-center gap-3">
                                            <span className="num text-xs text-muted-foreground">
                                                {permissionCountLabel}
                                            </span>
                                            {canEditRoles ? (
                                                <Link
                                                    id={`account-detail-role-${toAutomationIdSegment(roleId)}`}
                                                    href={`/system/roles/${encodeURIComponent(roleId)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${account.id}`)}`}
                                                    aria-label={`查看${role.name}的权限`}
                                                    title={`查看${role.name}的权限`}
                                                    className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-sidebar-accent/70 focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    <span>查看角色</span>
                                                    <ArrowUpRightIcon
                                                        className="size-3.5 shrink-0"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                            ) : null}
                                        </div>
                                    </li>
                                )
                            })}
                        </ul>
                    )}
                </DetailWorkspaceSection>
            ) : null}

            {canReadScopes ? (
                <DetailWorkspaceSection
                    title="数据范围"
                    headingId="account-profile-scopes-title"
                    width="full"
                    badge={
                        scopeSummary ? (
                            <div className="flex flex-wrap items-center gap-1.5">
                                <Badge variant="secondary" className="num">
                                    {scopeSummary.configurable.length} 项可配置
                                </Badge>
                                {scopeSummary.unconfiguredCount > 0 ? (
                                    <StatusBadge
                                        tone="warning"
                                        label={`${scopeSummary.unconfiguredCount} 项待配置`}
                                    />
                                ) : null}
                            </div>
                        ) : null
                    }
                    actions={
                        <Button
                            id="account-profile-open-scopes"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={() => onOpenScopes()}
                        >
                            管理全部范围
                            <ChevronRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    }
                >
                    {scopes.isError ? (
                        <BusinessFailureState
                            id="account-profile-scopes-retry"
                            error={scopes.error}
                            onRetry={() => void scopes.refetch()}
                        />
                    ) : scopes.isPending && !scopes.data ? (
                        <div className="space-y-2.5" aria-busy="true">
                            <div className="h-11 animate-pulse rounded-md bg-muted/70" />
                            <div className="h-11 animate-pulse rounded-md bg-muted/55" />
                            <div className="h-11 animate-pulse rounded-md bg-muted/40" />
                        </div>
                    ) : !scopeSummary || scopeSummary.all.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            尚无需要说明的数据访问政策，请先核对有效角色。
                        </p>
                    ) : visibleScopes.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            当前共 {scopeSummary.automatic.length}{" "}
                            项关联业务，均按岗位共享与业务规则自动校验，无需单独配置范围。
                        </p>
                    ) : (
                        <div className="space-y-3">
                            <ul className="divide-y divide-border/60 border-y border-border/60">
                                {visibleScopes.map((item) => {
                                    const segment = toAutomationIdSegment(
                                        item.business.resource,
                                    )
                                    const strongBadge =
                                        item.posture.kind === "unconfigured" ||
                                        item.posture.kind === "company"
                                    return (
                                        <li key={item.business.resource}>
                                            <button
                                                id={`account-overview-scope-${segment}`}
                                                type="button"
                                                onClick={() =>
                                                    onOpenScopes(
                                                        item.business.resource,
                                                    )
                                                }
                                                className="group flex w-full cursor-pointer items-center justify-between gap-4 py-2.5 text-left transition-colors hover:bg-muted/35 focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                <div className="flex min-w-0 flex-1 flex-col gap-0.5 sm:flex-row sm:items-baseline sm:gap-4">
                                                    <span className="shrink-0 font-medium text-foreground sm:w-36">
                                                        {resourceLabel(
                                                            item.business
                                                                .resource,
                                                        )}
                                                    </span>
                                                    <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                                                        {item.summary}
                                                    </span>
                                                </div>
                                                <div className="flex shrink-0 items-center gap-2">
                                                    <StatusBadge
                                                        tone={item.posture.tone}
                                                        label={
                                                            item.posture
                                                                .shortLabel
                                                        }
                                                        appearance={
                                                            strongBadge
                                                                ? "badge"
                                                                : "quiet"
                                                        }
                                                    />
                                                    <ChevronRightIcon
                                                        className="size-3.5 text-muted-foreground/50 transition-colors group-hover:text-foreground"
                                                        aria-hidden="true"
                                                    />
                                                </div>
                                            </button>
                                        </li>
                                    )
                                })}
                            </ul>
                            {scopeSummary.automatic.length > 0 ||
                            prioritizedScopes.length > OVERVIEW_SCOPE_LIMIT ? (
                                <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                    {scopeSummary.automatic.length > 0 ? (
                                        <span>
                                            另有 {scopeSummary.automatic.length}{" "}
                                            项业务按岗位共享与业务规则自动校验。
                                        </span>
                                    ) : (
                                        <span />
                                    )}
                                    {prioritizedScopes.length >
                                    OVERVIEW_SCOPE_LIMIT ? (
                                        <Button
                                            id="account-overview-scopes-more"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                                            onClick={() => onOpenScopes()}
                                        >
                                            查看全部 {prioritizedScopes.length}{" "}
                                            项可配置业务
                                            <ChevronRightIcon
                                                data-icon="inline-end"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}
                        </div>
                    )}
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
