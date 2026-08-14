/**
 * 权限目录：由 build.rs 生成的 PERMISSION_GROUPS 派生，按分组与维度归类。
 * 纯数据与纯函数，供权限面板组件与状态 hook 共用。
 */

import { PERMISSION_GROUPS } from "@/lib/permissions.generated"

export type PermissionItemOption = {
    /** 后端权限字符串（resource:action）。 */
    code: string
    description: string
    method: string
    path: string
}

export type PermissionGroupOption = {
    name: string
    description: string
    items: PermissionItemOption[]
}

/** 归属「系统」维度的权限组名（平台 / 治理 / 访问控制类）；其余归「业务」。 */
const SYSTEM_GROUP_NAMES = new Set([
    "账号管理",
    "角色管理",
    "系统审计",
    "来源注册",
    "单据注册",
    "统一待办",
    "批量任务",
    "文件资产",
    "权限与审计",
    "集成治理",
])

export type PermissionPanelTab = "business" | "system"

export const PERMISSION_PANEL_TAB_LABEL: Record<PermissionPanelTab, string> = {
    business: "业务",
    system: "系统",
}

export function isSystemGroup(name: string): boolean {
    return SYSTEM_GROUP_NAMES.has(name)
}

/** 权限目录：由 build.rs 生成的 PERMISSION_GROUPS 派生，按分组与维度归类。 */
export const PERMISSION_CATALOG: readonly PermissionGroupOption[] =
    PERMISSION_GROUPS.map((group) => ({
        name: group.name,
        description: group.description,
        items: group.permissions.map((permission) => ({
            code: `${permission.permission.resource}:${permission.permission.action}`,
            description: permission.description,
            method: permission.method,
            path: permission.path,
        })),
    }))

export const BUSINESS_GROUPS: readonly PermissionGroupOption[] =
    PERMISSION_CATALOG.filter((group) => !isSystemGroup(group.name))
export const SYSTEM_GROUPS: readonly PermissionGroupOption[] =
    PERMISSION_CATALOG.filter((group) => isSystemGroup(group.name))

export function matchesKeyword(item: PermissionItemOption, q: string): boolean {
    return [item.code, item.description, item.path]
        .join(" ")
        .toLowerCase()
        .includes(q)
}

/** 按关键词过滤组内权限项，仅保留含命中项的组；空关键词返回原数组（引用不变）。 */
export function filterGroupsByKeyword(
    groups: readonly PermissionGroupOption[],
    q: string,
): readonly PermissionGroupOption[] {
    if (!q) return groups
    return groups
        .map((group) => ({
            ...group,
            items: group.items.filter((item) => matchesKeyword(item, q)),
        }))
        .filter((group) => group.items.length > 0)
}

/** 统计各维度已选数量；不在目录中的编码忽略。 */
export function countSelectedByTab(
    selected: readonly string[],
): Record<PermissionPanelTab, number> {
    const counts: Record<PermissionPanelTab, number> = {
        business: 0,
        system: 0,
    }
    for (const code of selected) {
        const group = PERMISSION_CATALOG.find((candidate) =>
            candidate.items.some((item) => item.code === code),
        )
        if (!group) continue
        counts[isSystemGroup(group.name) ? "system" : "business"] += 1
    }
    return counts
}
