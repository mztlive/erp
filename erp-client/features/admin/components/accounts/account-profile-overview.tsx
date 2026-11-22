"use client"

import Link from "next/link"
import { ArrowUpRightIcon, KeyRoundIcon, PencilIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { adminAccountStatusBadge } from "../../lib/account-status"
import type { useRolesQuery } from "../../hooks/queries"
import type { AdminAccount } from "../../types"

export function AccountProfileOverview({
    account,
    roles,
    departmentLabel,
    canEdit,
    editDisabledReason,
    canChangePassword,
    canReadRoles,
    canEditRoles,
    onEdit,
    onPasswordChange,
}: {
    account: AdminAccount
    roles: ReturnType<typeof useRolesQuery>
    departmentLabel: string
    canEdit: boolean
    editDisabledReason?: string
    canChangePassword: boolean
    canReadRoles: boolean
    canEditRoles: boolean
    onEdit: () => void
    onPasswordChange: () => void
}) {
    const roleCount = account.role_ids.length
    const initial = Array.from(account.name.trim())[0]?.toUpperCase() ?? "?"

    return (
        <DetailWorkspaceSection
            title="账号资料"
            headingId="account-profile-title"
            subtitle="人员姓名、所属部门与角色统一保存后生效。"
            width="form"
            contentClassName="text-body-compact"
            actions={
                canEdit ? (
                    <Button
                        id="account-profile-edit"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        disabled={Boolean(editDisabledReason)}
                        title={editDisabledReason ?? "编辑账号资料"}
                        onClick={onEdit}
                    >
                        <PencilIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        编辑资料
                    </Button>
                ) : null
            }
        >
            <div>
                <div className="flex flex-col gap-4 pb-5 lg:flex-row lg:items-center">
                    <div className="flex min-w-0 items-center gap-3 lg:w-64 lg:shrink-0">
                        <span
                            aria-hidden="true"
                            className="flex size-11 shrink-0 items-center justify-center rounded-full bg-primary/10 text-body-compact font-semibold text-primary"
                        >
                            {initial}
                        </span>
                        <div className="min-w-0">
                            <p className="truncate text-body-compact leading-6 font-semibold">
                                {account.name}
                            </p>
                            <p className="mt-0.5 truncate text-xs text-muted-foreground">
                                登录账号 {account.account}
                            </p>
                        </div>
                    </div>
                    <dl className="grid min-w-0 flex-1 gap-x-8 gap-y-3 sm:grid-cols-3">
                        <div className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                所属部门
                            </dt>
                            <dd className="mt-0.5 break-words">
                                {departmentLabel}
                            </dd>
                        </div>
                        <div className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                创建时间
                            </dt>
                            <dd className="num mt-0.5">
                                {formatDateTime(
                                    new Date(
                                        account.created_at * 1000,
                                    ).toISOString(),
                                    "full",
                                )}
                            </dd>
                        </div>
                        <div className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                账号状态
                            </dt>
                            <dd className="mt-1">
                                <StatusBadge
                                    {...adminAccountStatusBadge(account.status)}
                                />
                            </dd>
                        </div>
                    </dl>
                </div>

                <div className="grid border-t border-border/60 sm:grid-cols-2 sm:divide-x sm:divide-border/60">
                    <section
                        aria-labelledby="account-profile-roles-title"
                        className="min-w-0 space-y-2 py-5 sm:pr-5"
                    >
                        <div className="flex items-center justify-between gap-2">
                            <h3
                                id="account-profile-roles-title"
                                className="text-xs font-medium text-muted-foreground"
                            >
                                角色
                            </h3>
                            {roleCount > 0 ? (
                                <span className="num text-xs text-muted-foreground">
                                    {roleCount} 个
                                </span>
                            ) : null}
                        </div>
                        {!canReadRoles && roleCount > 0 ? null : canReadRoles &&
                          roles.isError ? (
                            <BusinessFailureState
                                id="account-detail-roles-retry"
                                error={roles.error}
                                onRetry={() => void roles.refetch()}
                            />
                        ) : canReadRoles && roles.isPending ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                正在读取角色…
                            </p>
                        ) : !roleCount ? (
                            <div className="py-2">
                                <p className="font-medium">尚未分配角色</p>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    没有角色的账号无法执行任何业务操作。
                                    {canEdit && "可点击“编辑资料”分配。"}
                                </p>
                            </div>
                        ) : (
                            <ul className="flex flex-wrap gap-2">
                                {account.role_ids.map((roleId) => {
                                    const role = roles.data?.find(
                                        (row) => row.id === roleId,
                                    )
                                    if (!role) {
                                        return (
                                            <li
                                                key={roleId}
                                                className="min-w-0"
                                            >
                                                <span className="inline-flex max-w-full items-center rounded-md border border-dashed border-border px-2.5 py-1.5 text-xs text-muted-foreground">
                                                    角色信息待确认
                                                </span>
                                            </li>
                                        )
                                    }
                                    if (!canEditRoles) {
                                        return (
                                            <li
                                                key={roleId}
                                                className="min-w-0"
                                            >
                                                <span className="inline-flex max-w-full items-center rounded-md border border-border/70 bg-card px-2.5 py-1.5 text-xs font-medium">
                                                    <span className="truncate">
                                                        {role.name}
                                                    </span>
                                                </span>
                                            </li>
                                        )
                                    }
                                    return (
                                        <li key={roleId} className="min-w-0">
                                            <Link
                                                id={`account-detail-role-${toAutomationIdSegment(roleId)}`}
                                                href={`/system/roles/${encodeURIComponent(roleId)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${account.id}`)}`}
                                                aria-label={`查看${role.name}的权限`}
                                                title={`查看${role.name}的权限`}
                                                className="group inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1.5 text-xs font-medium transition-colors hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                            >
                                                <span className="truncate">
                                                    {role.name}
                                                </span>
                                                <ArrowUpRightIcon
                                                    className="size-3 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                        </li>
                                    )
                                })}
                            </ul>
                        )}
                        <p className="text-xs text-muted-foreground">
                            角色决定可以执行的操作；可访问的数据在“数据范围”中设置。
                        </p>
                    </section>

                    <section
                        aria-labelledby="account-profile-security-title"
                        className="min-w-0 space-y-2 border-t border-border/60 py-5 sm:border-t-0 sm:pl-5"
                    >
                        <h3
                            id="account-profile-security-title"
                            className="text-xs font-medium text-muted-foreground"
                        >
                            账号安全
                        </h3>
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <div className="min-w-0">
                                <p className="font-medium">登录密码</p>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    修改密码单独保存，不影响其他资料。
                                </p>
                            </div>
                            {canChangePassword ? (
                                <Button
                                    id="account-detail-password"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={onPasswordChange}
                                >
                                    <KeyRoundIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    修改密码
                                </Button>
                            ) : null}
                        </div>
                    </section>
                </div>
            </div>
        </DetailWorkspaceSection>
    )
}
