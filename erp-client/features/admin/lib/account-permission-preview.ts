import { GROUP_NAME_BY_CODE, permissionLabel } from "./permission-catalog"
import type { AdminAccount, AdminRole } from "../types"

export type AccountPermissionItem = {
    code: string
    label: string
    sources: { id: string; name: string }[]
}

/** 按已绑定角色整理配置来源；去重只用于展示，不在客户端作最终鉴权。 */
export function accountPermissionGroups(
    account: AdminAccount,
    roles: readonly AdminRole[],
) {
    const assigned = roles.filter((role) => account.role_ids.includes(role.id))
    const byCode = new Map<string, AccountPermissionItem>()
    for (const role of assigned) {
        for (const code of new Set(role.permissions)) {
            const item = byCode.get(code) ?? {
                code,
                label: permissionLabel(code),
                sources: [],
            }
            item.sources.push({ id: role.id, name: role.name })
            byCode.set(code, item)
        }
    }
    const groups = new Map<string, AccountPermissionItem[]>()
    for (const item of byCode.values()) {
        const name =
            item.code === "*:*"
                ? "全部模块"
                : (GROUP_NAME_BY_CODE.get(item.code) ?? "其它权限")
        groups.set(name, [...(groups.get(name) ?? []), item])
    }
    return {
        assigned,
        missingRoleCount: account.role_ids.filter(
            (id) => !roles.some((role) => role.id === id),
        ).length,
        allPermissions: byCode.has("*:*"),
        permissionCount: byCode.size,
        groups: [...groups].map(([name, items]) => ({ name, items })),
    }
}
