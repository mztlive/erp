import type { PersonalBusinessGrant } from "../api/personal-business-grants"
import { hasPermission } from "@/lib/permissions"
import { resourceLabel, actionLabel } from "@/lib/permission-catalog"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import { isRelationActive } from "@/features/organization/lib/tree"
import type { ScopeRule } from "@/features/organization/lib/scope-description"
import type { OrganizationStateView } from "@/features/organization/types"
import type { AdminAccount, AdminRole } from "../types"

export type SetupTask = {
    key: string
    message: string
    kind: "role" | "department" | "management"
    roleId?: string
}

/** 仅列配置待办，不宣称具体业务对象已授权。 */
export function accountSetupTasks(
    account: AdminAccount,
    roles: readonly AdminRole[],
    scopes: readonly ScopeRule[],
    org: OrganizationStateView,
    additions: readonly PersonalBusinessGrant[] = [],
): SetupTask[] {
    const tasks: SetupTask[] = []
    if (account.role_ids.length === 0) {
        tasks.push({
            key: "no-role",
            kind: "role",
            message: "尚未分配角色，请先编辑资料与角色。",
        })
    }
    const person = org.people.find((row) => row.id === account.id)
    for (const roleId of account.role_ids) {
        const role = roles.find((row) => row.id === roleId)
        if (!role) {
            tasks.push({
                key: roleId,
                kind: "role",
                message: "角色信息未完整返回，请刷新后核对。",
            })
            continue
        }
        const rules = scopes.filter(
            (row) =>
                row.subject_type === "role" &&
                row.subject_id === roleId &&
                row.enabled !== false,
        )
        const missing = registeredResources().flatMap((resource) =>
            resource.actions
                .filter(
                    (action) =>
                        hasPermission(
                            role.permissions,
                            `${resource.resource}:${action}`,
                        ) &&
                        !additions.some(
                            (grant) =>
                                grant.user_id === account.id &&
                                grant.role_id === roleId &&
                                grant.resource === resource.resource &&
                                grant.active_actions.includes(action) &&
                                grant.org_unit_ids.some((id) =>
                                    org.units.some(
                                        (unit) =>
                                            unit.id === id && unit.enabled,
                                    ),
                                ),
                        ) &&
                        !rules.some(
                            (rule) =>
                                rule.resource === resource.resource &&
                                rule.actions?.includes(action),
                        ),
                )
                .map(
                    (action) =>
                        `${resourceLabel(resource.resource)}·${actionLabel(action)}`,
                ),
        )
        if (missing.length)
            tasks.push({
                key: `${roleId}-scope`,
                kind: "role",
                roleId,
                message: `${role.name}：${missing.slice(0, 3).join("、")}${missing.length > 3 ? `等 ${missing.length} 项操作` : ""}尚未配置角色范围。合法历史读取另行判断。`,
            })
        const activeRules = rules.filter((rule) =>
            rule.actions?.some((action) =>
                hasPermission(role.permissions, `${rule.resource}:${action}`),
            ),
        )
        if (
            activeRules.some((rule) => rule.target_mode === "own_org") &&
            !person?.own_org_unit_id
        )
            tasks.push({
                key: `${roleId}-department`,
                kind: "department",
                roleId,
                message: `${role.name}使用“本人所属部门”，当前部门尚未设置或不在可查看范围。`,
            })
        if (
            activeRules.some((rule) => rule.target_mode === "managed_orgs") &&
            !org.management.some(
                (grant) =>
                    grant.user_id === account.id &&
                    grant.role_id === roleId &&
                    isRelationActive(
                        grant.valid_from,
                        grant.valid_to,
                        org.asOf,
                    ) &&
                    org.units.some(
                        (unit) => unit.id === grant.org_unit_id && unit.enabled,
                    ),
            )
        )
            tasks.push({
                key: `${roleId}-management`,
                kind: "management",
                roleId,
                message: `${role.name}使用“本人管理的部门”，当前可查看范围内未找到对应的有效管理关系。`,
            })
    }
    return tasks
}
