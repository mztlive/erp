import { resourceLabel } from "@/lib/permission-catalog"

/** 摘要来自实际权限，不按角色名称猜测职责。 */
export function roleCapabilitySummary(permissions?: readonly string[]): string {
    if (!permissions) return "操作权限信息待确认"
    if (permissions.includes("*:*") || permissions.includes("*"))
        return "全部操作权限；请谨慎分配，并单独核对数据范围。"
    const groups = new Map<string, number>()
    for (const permission of permissions) {
        const [resource, action] = permission.split(":")
        if (!resource || !action) continue
        groups.set(resource, (groups.get(resource) ?? 0) + 1)
    }
    if (groups.size === 0) return "尚未授予操作权限"
    const commonResources = new Set([
        "approval_instance",
        "background_job",
        "background_job_item",
        "file_asset",
        "work_item",
    ])
    const labels = [...groups]
        .sort(
            ([left, leftCount], [right, rightCount]) =>
                Number(commonResources.has(left)) -
                    Number(commonResources.has(right)) ||
                rightCount - leftCount,
        )
        .slice(0, 3)
        .map(([resource]) => resourceLabel(resource))
    return (
        labels.join("、") +
        (groups.size > 3 ? `等 ${groups.size} 类业务权限` : "操作权限")
    )
}
