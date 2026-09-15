"use client"

import { Button } from "@/components/ui/button"
import { KIND_LABEL } from "@/features/organization/lib/labels"
import { personLabel, roleLabel } from "@/features/organization/lib/tree"
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
        <section className="min-w-0 space-y-4 overflow-x-hidden">
            <div className="min-w-0 space-y-1">
                <h2 className="text-lg font-semibold wrap-anywhere">{unit.name}</h2>
                <p className="text-sm text-muted-foreground">
                    {KIND_LABEL[unit.kind]} · {unit.enabled ? "启用" : "停用"}
                </p>
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
                    <Button
                        id={`organization-unit-${segment}-transfer`}
                        type="button"
                        size="sm"
                        disabled={!unit.enabled}
                        onClick={() =>
                            onChange("transfer_member", { orgUnitId: unit.id })
                        }
                    >
                        成员调岗
                    </Button>
                    <Button
                        id={`organization-unit-${segment}-grant`}
                        type="button"
                        size="sm"
                        disabled={!unit.enabled}
                        onClick={() =>
                            onChange("grant_management", { orgUnitId: unit.id })
                        }
                    >
                        授予管理范围
                    </Button>
                </div>
            ) : null}

            <div className="min-w-0 space-y-2">
                <h3 className="text-sm font-medium">成员</h3>
                {node.members.length === 0 ? (
                    <p className="text-sm text-muted-foreground">当前没有主属成员。</p>
                ) : (
                    <ul className="min-w-0 space-y-2">
                        {node.members.map((member) => (
                            <li
                                key={member.id}
                                className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
                            >
                                <span className="min-w-0 wrap-anywhere">
                                    {personLabel(view.people, member.user_id)}
                                    <span className="mt-1 block text-xs text-muted-foreground">
                                        {instantLabel(member.valid_from)} 起 ·{" "}
                                        {instantLabel(member.valid_to)}
                                    </span>
                                </span>
                                {canManage ? (
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

            <div className="min-w-0 space-y-2">
                <h3 className="text-sm font-medium">管理授权</h3>
                {node.management.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        当前没有显式管理授权。部门负责人身份不会自动获得组织配置权。
                    </p>
                ) : (
                    <ul className="min-w-0 space-y-2">
                        {node.management.map((grant) => (
                            <li
                                key={grant.id}
                                className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
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
                                {canManage ? (
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
