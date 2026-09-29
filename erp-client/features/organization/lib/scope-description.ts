import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import type { DataScopeRecord, OrgUnit } from "../types"

/** 保留每条规则的业务、动作、维度和来源，不能用摘要替代鉴权。 */
export type ScopeRule = {
    id: string
    subject_type: "user" | "role"
    subject_id: string
    scope_type: string
    scope_targets: string[]
    resource: string
    actions?: string[]
    target_dimension?: string
    target_mode?: string | null
    include_descendants?: boolean | null
    enabled?: boolean
}

export function asScopeRule(row: DataScopeRecord): ScopeRule {
    return {
        id: row.id,
        subject_type: row.subjectType,
        subject_id: row.subjectId,
        scope_type: row.scopeType,
        scope_targets: row.scopeTargets,
        resource: row.resource,
        actions: row.actions,
        target_dimension: row.targetDimension,
        target_mode: row.targetMode,
        include_descendants: row.includeDescendants,
        enabled: row.enabled,
    }
}

export function scopeDescription(
    rule: ScopeRule,
    units: readonly OrgUnit[] = [],
): string {
    if (rule.enabled === false)
        return `已停用 · ${scopeDescription({ ...rule, enabled: true }, units)}`
    if (rule.scope_type === "company") return "公司范围"
    if (rule.scope_type === "self_owned") return "本人负责"
    if (rule.scope_type === "collaborative")
        return ["customer", "contract", "sales_order"].includes(rule.resource)
            ? "协作参与不授予该业务访问权"
            : "按业务协作资格判断"
    if (rule.target_mode === "managed_orgs")
        return "本人管理的部门（下级范围按管理关系）"
    const descendants = rule.include_descendants
        ? "（含下级部门）"
        : "（仅本级）"
    if (rule.target_mode === "own_org") return `本人所属部门${descendants}`
    if (rule.target_dimension === "warehouse")
        return `指定 ${rule.scope_targets.length} 个仓库`
    if (rule.target_dimension === "settlement_party")
        return `指定 ${rule.scope_targets.length} 个结算主体`
    if (rule.target_mode === "explicit")
        return (
            rule.scope_targets
                .map(
                    (id) =>
                        units.find((unit) => unit.id === id)?.name ??
                        "部门名称待确认",
                )
                .join("、") + descendants
        )
    return "范围条件未完整返回，请刷新后查看"
}

export function scopeCapability(rule: ScopeRule): string {
    return `${resourceLabel(rule.resource)}：${rule.actions?.map(actionLabel).join("、") || "动作待确认"}`
}
