/** 审计动作标签来自事件注册表；未知历史原值必须明确标记。 */

import { resourceLabel } from "@/features/admin/lib/permission-catalog"
import { AUDIT_ACTION_OPTIONS } from "@/lib/audit-actions.generated"
export { AUDIT_ACTION_OPTIONS }

/** 审计对象类型 → 中文对象名；未知类型原样返回。 */
export function auditObjectTypeLabel(objectType: string): string {
    return resourceLabel(objectType)
}

const AUDIT_ACTION_LABEL = new Map<string, string>(
    AUDIT_ACTION_OPTIONS.map((option) => [option.value, option.label]),
)

/** 列表和详情与筛选回显使用同一注册口径。 */
export function auditActionLabel(actionType: string): string {
    return registeredAuditActionLabel(actionType)
}

/** 未登记历史值只作明确标记的回显，不推导为合法事件。 */
export function registeredAuditActionLabel(actionType: string): string {
    return AUDIT_ACTION_LABEL.get(actionType) ?? `未知历史动作（${actionType}）`
}

/** 中文动作检索复用显示标签，转换为服务端关键词 OR 匹配的动作代码。 */
export function auditKeywordActions(q: string | undefined): string | undefined {
    const needle = q?.trim().toLowerCase()
    if (!needle) return undefined
    return (
        AUDIT_ACTION_OPTIONS.filter((option) =>
            option.label.toLowerCase().includes(needle),
        )
            .map((option) => option.value)
            .join(",") || undefined
    )
}
