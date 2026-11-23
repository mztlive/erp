"use client"

import Link from "next/link"
import {
    ArrowUpRightIcon,
    Building2Icon,
    KeyRoundIcon,
    ShieldIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { KIND_LABEL } from "@/features/organization/lib/labels"
import { personLabel } from "@/features/organization/lib/tree"
import type { OrganizationStateView } from "@/features/organization/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import {
    activeOrgMembership,
    activeUnitMemberCount,
    ledOrgUnits,
    orgUnitPath,
} from "../../lib/account-detail-view"
import type { AdminAccount } from "../../types"

/**
 * 人员基本资料右侧摘要栏（无套娃卡片，通过 1px 竖细线与主列分栏）：
 * 唯一承载组织归属上下文与账号安全控制，避免与页头或主列重复呈现相同信息。
 */
export function AccountDetailSidebar({
    account,
    isCurrentUser,
    hasSystemRole,
    canChangePassword,
    canToggleStatus,
    canEditProfile,
    canReadOrg,
    canManageOrg,
    orgData,
    orgPending,
    orgError,
    orgFailure,
    onRetryOrg,
    onPasswordChange,
    onEditProfile,
}: {
    account: AdminAccount
    isCurrentUser: boolean
    hasSystemRole: boolean
    canChangePassword: boolean
    canToggleStatus: boolean
    canEditProfile: boolean
    canReadOrg: boolean
    canManageOrg: boolean
    orgData: OrganizationStateView | undefined
    orgPending: boolean
    orgError: boolean
    orgFailure?: unknown
    onRetryOrg: () => void
    onPasswordChange: () => void
    onEditProfile: () => void
}) {
    const person = orgData?.people.find((row) => row.id === account.id)
    const ownUnitId = person?.own_org_unit_id ?? null
    const pathUnits =
        orgData && ownUnitId ? orgUnitPath(orgData.units, ownUnitId) : []
    const currentUnit = pathUnits[pathUnits.length - 1]
    const membership = orgData
        ? activeOrgMembership(orgData.memberships, account.id, orgData.asOf)
        : undefined
    const memberCount =
        orgData && currentUnit
            ? activeUnitMemberCount(
                  orgData.memberships,
                  currentUnit.id,
                  orgData.asOf,
              )
            : null
    const ledUnits = orgData ? ledOrgUnits(orgData.units, account.id) : []

    const protectionNote = hasSystemRole
        ? "绑定系统内置角色，不可停用或删除。"
        : isCurrentUser && canToggleStatus
          ? "当前登录账号不可停用。"
          : undefined

    return (
        <div className="min-w-0 space-y-6">
            {canReadOrg ? (
                <section aria-label="组织归属" className="space-y-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                            组织归属
                        </h2>
                        <Button
                            id="account-sidebar-open-org"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            render={
                                <Link
                                    href={
                                        currentUnit
                                            ? `/system/organization?unitId=${encodeURIComponent(currentUnit.id)}`
                                            : "/system/organization"
                                    }
                                />
                            }
                        >
                            组织架构
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    </div>

                    {orgError ? (
                        <BusinessFailureState
                            id="account-profile-org-retry"
                            error={orgFailure}
                            onRetry={onRetryOrg}
                        />
                    ) : orgPending && !orgData ? (
                        <div className="space-y-2" aria-busy="true">
                            <div className="h-4 w-28 animate-pulse rounded bg-muted" />
                            <div className="h-3.5 w-44 animate-pulse rounded bg-muted/70" />
                            <div className="h-3.5 w-36 animate-pulse rounded bg-muted/50" />
                        </div>
                    ) : !person ? (
                        <p className="text-xs text-muted-foreground">
                            此人不在你当前的组织查看范围内。
                        </p>
                    ) : currentUnit ? (
                        <div className="space-y-3">
                            <div className="space-y-1">
                                <div className="flex flex-wrap items-center gap-1.5">
                                    <Building2Icon
                                        className="size-3.5 shrink-0 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    <span className="font-medium text-foreground">
                                        {currentUnit.name}
                                    </span>
                                    <Badge variant="neutral">
                                        {KIND_LABEL[currentUnit.kind]}
                                    </Badge>
                                </div>
                                {pathUnits.length > 1 ? (
                                    <p className="text-xs text-muted-foreground">
                                        {pathUnits
                                            .map((unit) => unit.name)
                                            .join(" / ")}
                                    </p>
                                ) : null}
                            </div>

                            <dl className="space-y-2 text-body-compact">
                                <div className="flex items-baseline justify-between gap-3">
                                    <dt className="shrink-0 text-xs text-muted-foreground">
                                        直属负责人
                                    </dt>
                                    <dd className="min-w-0 truncate text-right text-foreground">
                                        {currentUnit.leader
                                            ? currentUnit.leader.user_id ===
                                              account.id
                                                ? "本人"
                                                : personLabel(
                                                      orgData!.people,
                                                      currentUnit.leader
                                                          .user_id,
                                                  )
                                            : "未设置"}
                                    </dd>
                                </div>
                                {memberCount != null ? (
                                    <div className="flex items-baseline justify-between gap-3">
                                        <dt className="shrink-0 text-xs text-muted-foreground">
                                            在编成员
                                        </dt>
                                        <dd className="num text-right text-foreground">
                                            {memberCount} 人
                                        </dd>
                                    </div>
                                ) : null}
                                {membership ? (
                                    <div className="flex items-baseline justify-between gap-3">
                                        <dt className="shrink-0 text-xs text-muted-foreground">
                                            归属生效
                                        </dt>
                                        <dd className="num text-right text-muted-foreground">
                                            {formatDateTime(
                                                new Date(
                                                    membership.valid_from *
                                                        1000,
                                                ).toISOString(),
                                                "full",
                                            )}
                                        </dd>
                                    </div>
                                ) : null}
                            </dl>
                        </div>
                    ) : (
                        <div className="space-y-2">
                            <p className="text-xs text-warning-soft-foreground">
                                尚未分配所属部门，可能影响按「本人所属部门」生效的数据范围。
                            </p>
                            {canEditProfile && canManageOrg ? (
                                <Button
                                    id="account-sidebar-assign-org"
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={onEditProfile}
                                >
                                    分配部门
                                </Button>
                            ) : null}
                        </div>
                    )}

                    {ledUnits.length > 0 ? (
                        <div className="space-y-1.5 border-t border-border/60 pt-3">
                            <p className="text-xs font-medium text-muted-foreground">
                                负责的组织（{ledUnits.length}）
                            </p>
                            <ul className="space-y-1">
                                {ledUnits.map((unit) => (
                                    <li key={unit.id}>
                                        <Link
                                            id={`account-sidebar-led-unit-${toAutomationIdSegment(unit.id)}`}
                                            href={`/system/organization?unitId=${encodeURIComponent(unit.id)}`}
                                            className="group flex items-center justify-between gap-2 rounded-md py-1 text-body-compact text-primary underline-offset-4 hover:underline"
                                        >
                                            <span className="min-w-0 truncate font-medium">
                                                {unit.name}
                                            </span>
                                            <span className="shrink-0 text-xs text-muted-foreground group-hover:text-primary">
                                                {KIND_LABEL[unit.kind]}
                                            </span>
                                        </Link>
                                    </li>
                                ))}
                            </ul>
                        </div>
                    ) : null}
                </section>
            ) : null}

            <section
                aria-labelledby="account-profile-security-title"
                className={
                    canReadOrg
                        ? "space-y-3 border-t border-border/60 pt-5"
                        : "space-y-3"
                }
            >
                <h2
                    id="account-profile-security-title"
                    className="text-section-title font-semibold tracking-tight text-foreground"
                >
                    账号安全
                </h2>

                <div className="space-y-2">
                    <div className="flex items-center justify-between gap-2">
                        <span className="font-medium text-foreground">
                            登录密码
                        </span>
                        {canChangePassword ? (
                            <Button
                                id="account-detail-password"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
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
                    <p className="text-xs text-muted-foreground">
                        修改密码单独保存，不影响姓名、部门与角色配置。
                    </p>
                </div>

                {protectionNote ? (
                    <p className="flex items-start gap-1.5 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                        <ShieldIcon
                            className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/80"
                            aria-hidden="true"
                        />
                        <span>{protectionNote}</span>
                    </p>
                ) : null}
            </section>
        </div>
    )
}
