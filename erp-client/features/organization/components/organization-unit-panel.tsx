"use client"

import { PlusIcon, ShieldCheckIcon, UsersIcon } from "lucide-react"

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

function instantLabel(secs: number | null): string {
    if (secs == null) return "持续有效"
    return formatDateTime(new Date(secs * 1000).toISOString(), "full")
}

export function OrganizationUnitPanel({
    node,
    view,
    canManage,
    onChange,
}: {
    node: OrgTreeNode
    view: OrganizationStateView
    canManage: boolean
    onChange: (operation: string, extras?: Record<string, string>) => void
}) {
    const unit = node.unit
    const segment = toAutomationIdSegment(unit.id)
    return (
        <section
            aria-label="组织资料"
            className="min-w-0 space-y-7 overflow-x-hidden px-4 py-6 sm:px-6 lg:px-7"
        >
            <div className="min-w-0 space-y-3">
                <p className="text-xs text-muted-foreground">
                    {unit.parent_id
                        ? (view.units.find((item) => item.id === unit.parent_id)
                              ?.name ?? "上级组织不可见")
                        : "一级组织"}{" "}
                    / {KIND_LABEL[unit.kind]}
                </p>
                <div className="flex flex-wrap items-center gap-3">
                    <h2 className="text-2xl font-semibold tracking-tight wrap-anywhere">
                        {unit.name}
                    </h2>
                    <BusinessStatusBadge
                        context="detail"
                        label={unit.enabled ? "启用" : "停用"}
                        tone={unit.enabled ? "success" : "neutral"}
                    />
                </div>
                <p className="text-sm text-muted-foreground">
                    主属成员{" "}
                    <span className="num text-foreground">
                        {node.members.length}
                    </span>{" "}
                    人<span className="mx-2 text-border">/</span>
                    管理授权{" "}
                    <span className="num text-foreground">
                        {node.management.length}
                    </span>{" "}
                    项
                </p>
                {!unit.enabled ? (
                    <p className="text-xs text-muted-foreground">
                        该组织已停用，不能新增下级、调入成员或授予管理范围。
                    </p>
                ) : null}
            </div>
            {canManage ? (
                <div className="flex min-w-0 flex-wrap gap-2">
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
                        停用
                    </Button>
                </div>
            ) : null}

            <div className="min-w-0 space-y-4 border-t border-border pt-6">
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
                            成员调岗
                        </Button>
                    ) : null}
                </div>
                {node.members.length === 0 ? (
                    <p className="rounded-lg bg-muted/30 px-4 py-8 text-center text-sm leading-6 text-muted-foreground">
                        当前没有主属成员。
                    </p>
                ) : (
                    <ul className="min-w-0 divide-y divide-border">
                        {node.members.map((member) => (
                            <li
                                key={member.id}
                                className="flex min-w-0 flex-wrap items-center justify-between gap-3 px-1 py-4 text-sm"
                            >
                                <span className="min-w-0 wrap-anywhere">
                                    {personLabel(view.people, member.user_id)}
                                    <span className="mt-1 block text-xs text-muted-foreground">
                                        {instantLabel(member.valid_from)} 起 ·{" "}
                                        {instantLabel(member.valid_to)}
                                    </span>
                                </span>
                                {canManage &&
                                isRelationActive(
                                    member.valid_from,
                                    member.valid_to,
                                    view.asOf,
                                ) ? (
                                    <Button
                                        id={`organization-member-${toAutomationIdSegment(member.id)}-end`}
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={() =>
                                            onChange("end_membership", {
                                                userId: member.user_id,
                                            })
                                        }
                                    >
                                        结束关系
                                    </Button>
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
                            授予管理范围
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
                                    {personLabel(view.people, grant.user_id)} ·{" "}
                                    {roleLabel(view.roles, grant.role_id)}
                                    <span className="mt-1 block text-xs text-muted-foreground">
                                        {grant.include_descendants
                                            ? "含下级"
                                            : "仅本级"}{" "}
                                        · {instantLabel(grant.valid_from)} 起
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
                                        撤销
                                    </Button>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        </section>
    )
}
