import { actionLabel } from "@/lib/permission-catalog"
import type { OrgUnit, ScopeDimension } from "@/features/organization/types"
import {
    personEffectiveDescription,
    type PersonScopeBusiness,
    type PersonScopeInput,
} from "../../api/person-data-scopes"

export const scopeDimensionLabel = (dimension: ScopeDimension) =>
    dimension === "internal_org"
        ? "部门"
        : dimension === "warehouse"
          ? "仓库"
          : "结算主体"

/** 按操作重算并集，移除一条授权后仍显示其他条目覆盖的范围。 */
export function ProposedScopeSummary({
    name,
    value,
    business,
    units,
}: {
    name: string
    value: PersonScopeInput
    business: PersonScopeBusiness
    units: OrgUnit[]
}) {
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
    return (
        <section
            className="space-y-2 rounded-md bg-muted/40 p-3"
            aria-live="polite"
        >
            <h3 className="font-medium">保存后的最终范围</h3>
            <p className="text-xs text-muted-foreground">
                {name}的各项操作分别合并基础范围与适用的追加授权。
            </p>
            {value.actions.map((action) => (
                <p key={action} className="text-xs leading-6">
                    <span className="font-medium">{actionLabel(action)}：</span>
                    {personEffectiveDescription(
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
                                alternatives: value.grants
                                    .filter((grant) =>
                                        grant.actions.includes(action),
                                    )
                                    .map((grant) => grant.terms),
                            },
                        },
                        business,
                        labels,
                    )}
                </p>
            ))}
            {value.editor && (
                <p className="text-xs text-amber-700">
                    正在编辑的授权还未加入列表，不计入以上范围。
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
