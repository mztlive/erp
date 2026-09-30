import type {
    CreateDataScopeInput,
    DataScopeRecord,
    ScopeDimension,
} from "@/features/organization/types"
import { meaningfulScope } from "./role-workbench"

export type ScopeChoice = {
    range: string
    dimension: ScopeDimension
    descendants: boolean
    targets: string[]
}

/** Only preselect a choice when every selected action has the same single effective range. */
export function initialScopeChoice(
    rows: readonly DataScopeRecord[],
    actions: readonly string[],
    dimension: ScopeDimension,
): ScopeChoice {
    const blank: ScopeChoice = {
        range: "",
        dimension,
        descendants: false,
        targets: [],
    }
    if (!actions.length) return blank
    const choices = actions.map((action) => {
        const matches = rows.filter(
            (row) => meaningfulScope(row) && row.actions.includes(action),
        )
        const values = matches.map((row): ScopeChoice => ({
            range:
                row.scopeType === "company" || row.scopeType === "self_owned"
                    ? row.scopeType
                    : (row.targetMode ?? ""),
            dimension: row.targetDimension,
            descendants: row.includeDescendants ?? false,
            targets: [...row.scopeTargets].sort(),
        }))
        const unique = [
            ...new Set(values.map((value) => JSON.stringify(value))),
        ]
        return unique.length === 1 ? unique[0] : null
    })
    return choices[0] && choices.every((value) => value === choices[0])
        ? (JSON.parse(choices[0]) as ScopeChoice)
        : blank
}

export function scopeChoiceInput(
    value: ScopeChoice,
    roleId: string,
    resource: string,
    actions: readonly string[],
): CreateDataScopeInput {
    const targeted = ["explicit", "own_org", "managed_orgs"].includes(
        value.range,
    )
    return {
        subjectType: "role",
        subjectId: roleId,
        resource,
        actions: [...actions],
        scopeType: targeted
            ? "organization"
            : value.range === "company"
              ? "company"
              : "self_owned",
        targetDimension: value.dimension,
        targetMode: targeted
            ? (value.range as "explicit" | "own_org" | "managed_orgs")
            : null,
        includeDescendants:
            targeted &&
            value.dimension === "internal_org" &&
            value.range !== "managed_orgs"
                ? value.descendants
                : null,
        scopeTargets: value.range === "explicit" ? value.targets : [],
    }
}
