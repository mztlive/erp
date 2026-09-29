"use client"
import Link from "next/link"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useDataScopesQuery } from "../hooks/queries"
import type { OrganizationChangeDraft } from "../lib/change-payload"
import type { OrganizationStateView } from "../types"
import { unitLabel } from "../lib/tree"
import { resourceLabel, actionLabel } from "@/lib/permission-catalog"

/** 解释组织变更触发的范围重算；不将配置预览冒充对象授权模拟。 */
export function OrganizationAccessImpact({
    draft,
    view,
}: {
    draft: OrganizationChangeDraft
    view: OrganizationStateView
}) {
    if (["transfer_member", "end_membership"].includes(draft.operation)) {
        const person = view.people.find((row) => row.id === draft.userId)
        return (
            <div className="space-y-2 rounded-md bg-muted/40 p-3 text-sm">
                <p className="font-medium">权限影响</p>
                <p>
                    {person?.own_org_unit_id
                        ? unitLabel(view.units, person.own_org_unit_id)
                        : "原部门待确认"}{" "}
                    →{" "}
                    {draft.operation === "end_membership"
                        ? "移出部门"
                        : draft.orgUnitId
                          ? unitLabel(view.units, draft.orgUnitId)
                          : "请选择新部门"}
                </p>
                <ul className="list-inside list-disc space-y-1 text-muted-foreground">
                    <li>使用“本人所属部门”的规则会按新部门重新计算。</li>
                    <li>
                        已设置的管理部门继续保留，请核对是否需要撤销或调整。
                    </li>
                    <li>
                        角色与本人负责的业务不会因调岗自动移交；历史业绩归属保持不变。
                    </li>
                </ul>
                {draft.userId && (
                    <Link
                        target="_blank"
                        rel="noopener noreferrer"
                        id="organization-impact-account"
                        className="block text-primary"
                        href={`/system/accounts/${encodeURIComponent(draft.userId)}`}
                    >
                        查看此人的角色、管理部门与访问检查
                    </Link>
                )}
            </div>
        )
    }
    if (draft.operation !== "grant_management" || !draft.roleId) return null
    return <ManagementAccessImpact draft={draft} />
}

function ManagementAccessImpact({ draft }: { draft: OrganizationChangeDraft }) {
    const profile = useAccountProfileQuery()
    const scopes = useDataScopesQuery(
        { subjectType: "role", subjectId: draft.roleId, scopeType: "all" },
        Boolean(draft.roleId) &&
            hasPermission(profile.data?.permissions, "data_scope:list"),
    )
    const rules =
        scopes.data?.items.filter(
            (row) => row.enabled && row.targetMode === "managed_orgs",
        ) ?? []
    return (
        <div className="space-y-2 rounded-md bg-muted/40 p-3 text-sm">
            <p className="font-medium">管理关系如何生效</p>
            <p>
                此人必须持有所选角色；该角色还须同时提供业务操作权限和“本人管理的部门”范围。
            </p>
            {scopes.isError ? (
                <p className="text-amber-700">
                    范围读取失败，不能判断已配置完成。
                </p>
            ) : scopes.isSuccess ? (
                rules.length ? (
                    <p>
                        使用此管理关系的范围规则：
                        {rules
                            .map(
                                (row) =>
                                    `${resourceLabel(row.resource)}（${row.actions.map(actionLabel).join("、")}）`,
                            )
                            .join("；")}
                        。具体操作权限仍需核对该角色。
                    </p>
                ) : (
                    <p className="text-amber-700">
                        该角色没有启用的“本人管理的部门”范围。保存管理关系不会自动扩大业务权限。
                    </p>
                )
            ) : (
                <p>范围配置待确认，请由有权限的管理员核对。</p>
            )}
            <Link
                target="_blank"
                rel="noopener noreferrer"
                id="organization-impact-role"
                className="block text-primary"
                href={`/system/roles/${encodeURIComponent(draft.roleId)}/edit`}
            >
                查看该角色操作与范围
            </Link>
        </div>
    )
}
