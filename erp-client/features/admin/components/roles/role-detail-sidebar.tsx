"use client"

import { ChevronRightIcon, ShieldAlertIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { roleCapabilitySummary } from "../../lib/role-summary"
import type { RolePermissionOverview } from "../../lib/role-detail-view"
import type { AdminRole } from "../../types"
import type { PermissionReviewMode } from "./role-permission-dialogs"

const SIDEBAR_MODULE_LIMIT = 6

/**
 * 角色基础设置右侧摘要栏（无套娃卡片，通过 1px 竖细线与主列分栏）：
 * 呈现当前权限覆盖态势、高风险提醒与角色治理边界，支持一键定位到对应权限模块。
 */
export function RoleDetailSidebar({
    role,
    permissions,
    overview,
    workbenchCount,
    boundAccountCount,
    canReadAdmins,
    onJumpToPermissions,
    onJumpToModule,
    onOpenMembers,
    onOpenReview,
}: {
    role: AdminRole
    permissions: readonly string[]
    overview: RolePermissionOverview
    workbenchCount: number
    boundAccountCount: number | null
    canReadAdmins: boolean
    onJumpToPermissions: () => void
    onJumpToModule: (groupName: string) => void
    onOpenMembers?: () => void
    onOpenReview: (mode: PermissionReviewMode) => void
}) {
    const visibleModules = overview.coveredGroups.slice(0, SIDEBAR_MODULE_LIMIT)
    const remainingModuleCount = Math.max(
        0,
        overview.coveredGroups.length - visibleModules.length,
    )

    return (
        <div className="min-w-0 space-y-6">
            <section aria-label="权限覆盖概览" className="space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        权限覆盖
                    </h2>
                    <Button
                        id="role-sidebar-open-permissions"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        onClick={onJumpToPermissions}
                    >
                        配置权限
                        <ChevronRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </div>

                {overview.hasWildcard ? (
                    <p className="text-xs text-muted-foreground">
                        拥有全部业务与系统操作权限；具体可访问的单据仍遵循人员数据范围与审批规则。
                    </p>
                ) : overview.effectiveCount === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        尚未配置操作权限，绑定此角色的账号将无法执行业务操作。
                    </p>
                ) : (
                    <div className="space-y-3">
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                            <p className="text-xs text-muted-foreground">
                                已授权{" "}
                                <span className="num font-semibold text-foreground">
                                    {overview.effectiveCount}
                                </span>{" "}
                                项，覆盖{" "}
                                <span className="num font-semibold text-foreground">
                                    {overview.coveredGroups.length}
                                </span>{" "}
                                个模块
                            </p>
                            {overview.dangerousCount > 0 ? (
                                <button
                                    id="role-sidebar-review-dangerous"
                                    type="button"
                                    onClick={() => onOpenReview("dangerous")}
                                    className="inline-flex cursor-pointer items-center gap-1 rounded-sm bg-destructive-soft px-1.5 py-0.5 text-tiny font-medium text-destructive-soft-foreground transition-colors hover:bg-destructive-soft/80 focus-visible:outline-2 focus-visible:outline-primary"
                                >
                                    <ShieldAlertIcon
                                        className="size-3 shrink-0"
                                        aria-hidden="true"
                                    />
                                    <span className="num">
                                        高风险 {overview.dangerousCount} 项
                                    </span>
                                </button>
                            ) : null}
                        </div>

                        <ul className="divide-y divide-border/50 border-y border-border/60">
                            {visibleModules.map((group) => (
                                <li key={group.name}>
                                    <button
                                        id={`role-sidebar-module-${toAutomationIdSegment(group.name)}`}
                                        type="button"
                                        onClick={() =>
                                            onJumpToModule(group.name)
                                        }
                                        className="group flex w-full cursor-pointer items-center justify-between gap-2 py-2 text-left text-body-compact transition-colors hover:text-primary focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-primary"
                                    >
                                        <span className="min-w-0 truncate text-foreground group-hover:text-primary">
                                            {group.name}
                                        </span>
                                        <span className="num shrink-0 text-xs text-muted-foreground group-hover:text-primary">
                                            {group.selected} / {group.total}
                                        </span>
                                    </button>
                                </li>
                            ))}
                        </ul>

                        {remainingModuleCount > 0 ? (
                            <Button
                                id="role-sidebar-review-selected"
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="h-auto px-0 text-primary hover:bg-transparent hover:underline"
                                onClick={() => onOpenReview("selected")}
                            >
                                另有 {remainingModuleCount}{" "}
                                个模块，查看全部已授权明细
                                <ChevronRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null}
                    </div>
                )}
            </section>

            <section
                aria-label="角色属性与保护"
                className="space-y-3 border-t border-border/60 pt-5"
            >
                <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                    角色属性
                </h2>
                <dl className="space-y-2.5 text-body-compact">
                    <div className="flex items-center justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            角色类型
                        </dt>
                        <dd className="text-right">
                            {role.system ? (
                                <Badge variant="indigo">系统内置</Badge>
                            ) : (
                                <span className="text-foreground">
                                    自定义角色
                                </span>
                            )}
                        </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            能力摘要
                        </dt>
                        <dd className="min-w-0 text-right text-xs text-foreground">
                            {roleCapabilitySummary(permissions)}
                        </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            岗位工作台
                        </dt>
                        <dd className="num text-right text-foreground">
                            {workbenchCount > 0
                                ? `${workbenchCount} 个`
                                : "未配置"}
                        </dd>
                    </div>
                    {canReadAdmins && boundAccountCount !== null ? (
                        <div className="flex items-baseline justify-between gap-3">
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                关联人员
                            </dt>
                            <dd className="text-right">
                                {onOpenMembers ? (
                                    <button
                                        id="role-sidebar-open-members"
                                        type="button"
                                        onClick={onOpenMembers}
                                        className="num cursor-pointer text-body-compact font-medium text-primary underline-offset-4 hover:underline focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-primary"
                                    >
                                        {boundAccountCount} 人
                                    </button>
                                ) : (
                                    <span className="num text-foreground">
                                        {boundAccountCount} 人
                                    </span>
                                )}
                            </dd>
                        </div>
                    ) : null}
                    {role.created_at ? (
                        <div className="flex items-baseline justify-between gap-3">
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                创建时间
                            </dt>
                            <dd className="num text-right text-muted-foreground">
                                {formatDateTime(
                                    new Date(
                                        role.created_at * 1000,
                                    ).toISOString(),
                                    "full",
                                )}
                            </dd>
                        </div>
                    ) : null}
                </dl>
            </section>
        </div>
    )
}
