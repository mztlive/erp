import { actionLabel } from "@/lib/permission-catalog"
import type { OrgUnit, ScopeDimension } from "@/features/organization/types"
import {
    personEffectiveDescription,
    personGrantKey,
    type PersonScopeBusiness,
    type PersonScopeInput,
} from "../../api/person-data-scopes"

export const scopeDimensionLabel = (dimension: ScopeDimension) =>
    dimension === "internal_org"
        ? "部门"
        : dimension === "warehouse"
          ? "仓库"
          : "结算主体"

/** 仅合并规范条件完全相同的操作，不能按可能重名的目标文案归组。 */
export function ProposedScopeSummary({
    value,
    business,
    units,
    incompleteKeys,
}: {
    value: PersonScopeInput
    business: PersonScopeBusiness
    units: OrgUnit[]
    incompleteKeys: Set<string>
}) {
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
    const groups = new Map<string, { actions: string[]; description: string }>()
    for (const action of value.actions) {
        const alternatives = value.grants
            .filter(
                (grant) =>
                    !incompleteKeys.has(grant.key) &&
                    grant.actions.includes(action),
            )
            .map((grant) => grant.terms)
        const company = alternatives.some((terms) =>
            terms.some((term) => term.scope_type === "company"),
        )
        const key = company
            ? "company"
            : JSON.stringify(
                  [...new Set(alternatives.map(personGrantKey))].sort(),
              )
        const group = groups.get(key) ?? {
            actions: [],
            description: personEffectiveDescription(
                {
                    id: "preview",
                    user_id: "",
                    version: 0,
                    created_at: 0,
                    resource: value.resource,
                    action,
                    expression: {
                        additive: true,
                        history_read: false,
                        condition: null,
                        alternatives: [
                            ...new Map(
                                alternatives.map((terms) => [
                                    personGrantKey(terms),
                                    terms,
                                ]),
                            ).values(),
                        ],
                    },
                },
                business,
                labels,
            ),
        }
        group.actions.push(action)
        groups.set(key, group)
    }
    return (
        <section className="space-y-2 border-t pt-4" aria-live="polite">
            <h3 className="text-xs font-medium text-muted-foreground">
                {incompleteKeys.size ? "已完善范围的预览" : "保存后可访问"}
            </h3>
            {[...groups.entries()].map(([key, group]) => (
                <p key={key} className="text-xs leading-6">
                    <span className="font-medium">
                        {group.actions.length === value.actions.length
                            ? "所有已有操作"
                            : group.actions.map(actionLabel).join("、")}
                        ：
                    </span>
                    {group.description}
                </p>
            ))}
            {incompleteKeys.size > 0 && (
                <p className="text-xs text-amber-700">
                    还有 {incompleteKeys.size}{" "}
                    条范围未完善，暂不计入预览，填写完整后才能保存。
                </p>
            )}
            {value.resource === "approval_instance" && (
                <p className="text-xs text-muted-foreground">
                    实际审批仍须符合流程节点要求；添加范围不会把此人安排为审批人。
                </p>
            )}
        </section>
    )
}
