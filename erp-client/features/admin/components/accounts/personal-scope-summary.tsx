import { actionLabel } from "@/lib/permission-catalog"
import type { OrgUnit, ScopeDimension } from "@/features/organization/types"
import {
    personScopeDescription,
    type PersonScopeInput,
    type PersonScopeList,
} from "../../api/person-data-scopes"

export const scopeDimensionLabel = (dimension: ScopeDimension) =>
    dimension === "internal_org"
        ? "部门"
        : dimension === "warehouse"
          ? "仓库"
          : "结算主体"

/** 已有操作范围相同时合并，范围不同时保留逐操作说明。 */
export function SavedScopeSummary({
    data,
    value,
    units,
}: {
    data: PersonScopeList
    value: PersonScopeInput
    units: OrgUnit[]
}) {
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
    const scopes = value.actions.map((action) =>
        data.items.find(
            (item) =>
                item.resource === value.resource && item.action === action,
        ),
    )
    if (!scopes.length) return null
    const same =
        new Set(scopes.map((scope) => JSON.stringify(scope?.expression)))
            .size === 1
    return (
        <div className="space-y-1 text-xs text-muted-foreground">
            {same ? (
                <p>
                    {scopes[0]
                        ? `这些操作当前使用：${personScopeDescription(scopes[0], labels)}`
                        : "这些操作尚未设置数据范围。"}
                </p>
            ) : (
                <>
                    <p>这些操作当前范围不同：</p>
                    {value.actions.map((action, index) => (
                        <p key={action}>
                            {actionLabel(action)}：
                            {personScopeDescription(scopes[index], labels)}
                        </p>
                    ))}
                </>
            )}
        </div>
    )
}

/** 仅描述待保存的配置，不代替实际操作资格或流程节点判断。 */
export function ProposedScopeSummary({
    name,
    value,
    dimensions,
    units,
    incomplete,
}: {
    name: string
    value: PersonScopeInput
    dimensions: ScopeDimension[]
    units: OrgUnit[]
    incomplete?: string
}) {
    const departmentNames = value.org_ids
        .map(
            (id) =>
                units.find((unit) => unit.id === id)?.name ??
                "名称待确认的部门",
        )
        .join("、")
    const descriptions =
        value.mode === "company"
            ? ["公司范围"]
            : dimensions.map((dimension) => {
                  if (dimension === "warehouse")
                      return `所选 ${value.warehouse_ids.length} 个仓库关联的数据`
                  if (dimension === "settlement_party")
                      return `所选 ${value.settlement_ids.length} 个结算主体关联的数据`
                  if (value.mode === "self") return `${name}负责的数据`
                  const departments =
                      value.mode === "own_org"
                          ? `${name}所属部门`
                          : departmentNames
                  return `${departments}${value.include_descendants ? "及下级部门" : ""}的数据`
              })
    return (
        <div
            className="space-y-2 rounded-md bg-muted/40 p-3"
            aria-live="polite"
        >
            <p className="font-medium">保存后的范围</p>
            {incomplete ? (
                <p className="text-muted-foreground">{incomplete}</p>
            ) : (
                <>
                    <p>
                        {name}的{value.actions.map(actionLabel).join("、")}
                        操作将使用：{descriptions.join("，并且同时属于")}。
                    </p>
                    <p className="text-xs text-muted-foreground">
                        仅替换以上操作的范围，未勾选操作保持原配置。
                    </p>
                </>
            )}
            {value.resource === "approval_instance" && (
                <p className="text-xs text-muted-foreground">
                    实际审批仍须符合流程节点要求；设置范围不会把此人安排为审批人。
                </p>
            )}
        </div>
    )
}
