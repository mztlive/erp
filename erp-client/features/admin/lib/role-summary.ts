import { resourceLabel, actionLabel } from "@/lib/permission-catalog"

/** 摘要来自实际权限，不按角色名称猜测职责。 */
export function roleCapabilitySummary(permissions?: readonly string[]): string {
    if (!permissions) return "操作权限信息待确认"
    if (permissions.includes("*:*") || permissions.includes("*"))
        return "全部操作权限；请谨慎分配，并单独核对数据范围。"
    const groups = new Map<string, string[]>()
    for (const permission of permissions) {
        const [resource, action] = permission.split(":")
        if (!resource || !action) continue
        const actions = groups.get(resource) ?? []
        actions.push(action === "*" ? "全部操作" : actionLabel(action))
        groups.set(resource, actions)
    }
    const labels = [...groups]
        .slice(0, 3)
        .map(
            ([resource, actions]) =>
                `${resourceLabel(resource)}：${actions.join("、")}`,
        )
    return (
        labels.join("；") +
            (groups.size > 3 ? `；另有 ${groups.size - 3} 类业务` : "") ||
        "尚未授予操作权限"
    )
}
