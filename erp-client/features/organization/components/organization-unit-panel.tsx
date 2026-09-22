"use client"

import Link from "next/link"

import {
    ArrowRightLeftIcon,
    BanIcon,
    FolderInputIcon,
    PencilIcon,
    PlusIcon,
    ShieldCheckIcon,
    ShieldMinusIcon,
    UserMinusIcon,
    UserPlusIcon,
    UsersIcon,
} from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import { KIND_LABEL } from "@/features/organization/lib/labels"
import {
    isRelationActive,
    personLabel,
    roleLabel,
} from "@/features/organization/lib/tree"
import type { OrgTreeNode } from "@/features/organization/lib/tree"
import type { OrganizationStateView } from "@/features/organization/types"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 空组织树初始化把生效起点写成 Unix 秒 1，表示从系统时点起有效，不是业务日期。 */
const OPEN_ENDED_START_SECS = 1

function clockLabel(secs: number): string {
    return formatDateTime(new Date(secs * 1000).toISOString(), "full")
}

function relationPeriod(from: number, to: number | null): string {
    const openStart = from <= OPEN_ENDED_START_SECS
    if (openStart && to == null) return "持续有效"
    if (openStart && to != null) return `至 ${clockLabel(to)}`
    if (to == null) return `${clockLabel(from)} 起 · 持续有效`
    return `${clockLabel(from)} 起 · ${clockLabel(to)}`
}

export function OrganizationUnitPanel({
    node,
    view,
    canManage,
    canViewAccounts = false,
    onChange,
}: {
    node: OrgTreeNode
    view: OrganizationStateView
    canViewAccounts?: boolean
    canManage: boolean
    onChange: (operation: string, extras?: Record<string, string>) => void
}) {
    const unit = node.unit
    const segment = toAutomationIdSegment(unit.id)
    const parentLabel = unit.parent_id
        ? (view.units.find((item) => item.id === unit.parent_id)?.name ??
          "上级组织不可见")
        : "一级组织"
    return (
        <section aria-label="组织资料" className="min-w-0 overflow-x-hidden">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border px-4 py-3 sm:px-6 lg:px-7">
                <div className="min-w-0">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                        <h2 className="text-base font-semibold tracking-tight wrap-anywhere">
                            {unit.name}
                        </h2>
                        <BusinessStatusBadge
                            context="list"
                            label={unit.enabled ? "启用" : "停用"}
                            tone={unit.enabled ? "success" : "neutral"}
                        />
                        <span className="text-xs text-muted-foreground">
                            {parentLabel} / {KIND_LABEL[unit.kind]}
                        </span>
                    </div>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        主属成员{" "}
                        <span className="num text-foreground">
                            {node.members.length}
                        </span>{" "}
                        人<span className="mx-1.5 text-border">/</span>
                        管理授权{" "}
                        <span className="num text-foreground">
                            {node.management.length}
                        </span>{" "}
                        项
                    </p>
                    {!unit.enabled ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            该组织已停用，不能新增下级、调入成员或设置管理部门。
                        </p>
                    ) : null}
                </div>
                {canManage ? (
                    <div className="flex min-w-0 flex-wrap gap-1.5">
                        <Button
                            id={`organization-unit-${segment}-create-child`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!unit.enabled}
                            onClick={() =>
                                onChange("create_unit", { parentId: unit.id })
                            }
                        >
                            <PlusIcon className="size-3.5" aria-hidden="true" />
                            新建下级
                        </Button>
                        <Button
                            id={`organization-unit-${segment}-rename`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!unit.enabled}
                            onClick={() =>
                                onChange("rename_unit", {
                                    orgUnitId: unit.id,
                                    name: unit.name,
                                })
                            }
                        >
                            <PencilIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            重命名
                        </Button>
                        <Button
                            id={`organization-unit-${segment}-move`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!unit.enabled}
                            onClick={() =>
                                onChange("move_unit", { orgUnitId: unit.id })
                            }
                        >
                            <FolderInputIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            移动
                        </Button>
                        <Button
                            id={`organization-unit-${segment}-disable`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!unit.enabled}
                            onClick={() =>
                                onChange("disable_unit", { orgUnitId: unit.id })
                            }
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            停用
                        </Button>
                    </div>
                ) : null}
            </div>

            <div className="min-w-0 px-4 py-4 sm:px-6 lg:px-7">
                <div className="min-w-0 space-y-4">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                            <UsersIcon
                                className="size-4 text-muted-foreground"
                                aria-hidden="true"
                            />
                            成员{" "}
                            <span className="num text-xs text-muted-foreground">
                                {node.members.length}
                            </span>
                        </h3>
                        {canManage ? (
                            <Button
                                id={`organization-unit-${segment}-transfer`}
                                type="button"
                                size="sm"
                                disabled={!unit.enabled}
                                onClick={() =>
                                    onChange("transfer_member", {
                                        orgUnitId: unit.id,
                                    })
                                }
                            >
                                <UserPlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                添加成员
                            </Button>
                        ) : null}
                    </div>
                    {node.members.length === 0 ? (
                        <p className="rounded-lg bg-muted/30 px-4 py-8 text-center text-sm leading-6 text-muted-foreground">
                            当前部门还没有成员，请点击「添加成员」选择已有账号。
                        </p>
                    ) : (
                        <ul className="min-w-0 divide-y divide-border">
                            {node.members.map((member) => (
                                <li
                                    key={member.id}
                                    className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 px-1 py-3 text-sm"
                                >
                                    <span className="flex min-w-0 flex-wrap items-baseline gap-x-2">
                                        {canViewAccounts ? (
                                            <Link
                                                id={`organization-member-${toAutomationIdSegment(member.id)}-account`}
                                                className="wrap-anywhere hover:text-primary"
                                                href={`/system/accounts?q=${encodeURIComponent(view.people.find((person) => person.id === member.user_id)?.account ?? "")}`}
                                            >
                                                {personLabel(
                                                    view.people,
                                                    member.user_id,
                                                )}
                                            </Link>
                                        ) : (
                                            <span className="wrap-anywhere">
                                                {personLabel(
                                                    view.people,
                                                    member.user_id,
                                                )}
                                            </span>
                                        )}
                                        <span className="text-xs text-muted-foreground">
                                            {relationPeriod(
                                                member.valid_from,
                                                member.valid_to,
                                            )}
                                        </span>
                                    </span>
                                    {canManage &&
                                    isRelationActive(
                                        member.valid_from,
                                        member.valid_to,
                                        view.asOf,
                                    ) ? (
                                        <div className="flex shrink-0 flex-wrap gap-0.5">
                                            <Button
                                                id={`organization-member-${toAutomationIdSegment(member.id)}-transfer`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                onClick={() =>
                                                    onChange(
                                                        "transfer_member",
                                                        {
                                                            userId: member.user_id,
                                                            orgUnitId: unit.id,
                                                        },
                                                    )
                                                }
                                            >
                                                <ArrowRightLeftIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                调整部门
                                            </Button>
                                            <Button
                                                id={`organization-member-${toAutomationIdSegment(member.id)}-end`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                onClick={() =>
                                                    onChange("end_membership", {
                                                        userId: member.user_id,
                                                    })
                                                }
                                            >
                                                <UserMinusIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                移出部门
                                            </Button>
                                        </div>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>

                <div className="min-w-0 space-y-4 border-t border-border pt-6">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                            <ShieldCheckIcon
                                className="size-4 text-muted-foreground"
                                aria-hidden="true"
                            />
                            管理授权{" "}
                            <span className="num text-xs text-muted-foreground">
                                {node.management.length}
                            </span>
                        </h3>
                        {canManage ? (
                            <Button
                                id={`organization-unit-${segment}-grant`}
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={!unit.enabled}
                                onClick={() =>
                                    onChange("grant_management", {
                                        orgUnitId: unit.id,
                                    })
                                }
                            >
                                <ShieldCheckIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                设置管理部门
                            </Button>
                        ) : null}
                    </div>
                    {node.management.length === 0 ? (
                        <p className="rounded-lg bg-muted/30 px-4 py-8 text-center text-sm leading-6 text-muted-foreground">
                            当前没有显式管理授权。部门负责人身份不会自动获得组织配置权。
                        </p>
                    ) : (
                        <ul className="min-w-0 divide-y divide-border">
                            {node.management.map((grant) => (
                                <li
                                    key={grant.id}
                                    className="flex min-w-0 flex-wrap items-center justify-between gap-3 px-1 py-4 text-sm"
                                >
                                    <span className="min-w-0 wrap-anywhere">
                                        {personLabel(
                                            view.people,
                                            grant.user_id,
                                        )}{" "}
                                        · {roleLabel(view.roles, grant.role_id)}
                                        <span className="mt-1 block text-xs text-muted-foreground">
                                            {grant.include_descendants
                                                ? "含下级"
                                                : "仅本级"}{" "}
                                            ·{" "}
                                            {relationPeriod(
                                                grant.valid_from,
                                                grant.valid_to,
                                            )}
                                        </span>
                                    </span>
                                    {canManage &&
                                    isRelationActive(
                                        grant.valid_from,
                                        grant.valid_to,
                                        view.asOf,
                                    ) ? (
                                        <Button
                                            id={`organization-grant-${toAutomationIdSegment(grant.id)}-revoke`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={() =>
                                                onChange("revoke_management", {
                                                    assignmentId: grant.id,
                                                })
                                            }
                                        >
                                            <ShieldMinusIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            撤销
                                        </Button>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            </div>
        </section>
    )
}
