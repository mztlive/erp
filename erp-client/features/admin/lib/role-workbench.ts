import {
    PERMISSION_BY_CODE,
    actionLabel,
    resourceLabel,
} from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import type { DataScopeRecord } from "@/features/organization/types"

const ACTIONS = [
    "list",
    "detail",
    "read",
    "get",
    "create",
    "update",
    "submit",
    "cancel_approval",
    "delete",
]
const PRIMARY = [
    "customer",
    "contract",
    "sales_order",
    "customer_acceptance",
    "purchase_order",
    "supplier",
]

/** Include every known action, even when only some actions consume data scopes. */
export function workbenchResources() {
    return registeredResources()
        .map((entry) => ({
            ...entry,
            codes: [...PERMISSION_BY_CODE.keys()]
                .filter((code) => code.startsWith(`${entry.resource}:`))
                .sort((a, b) => {
                    const rank = (code: string) => {
                        const index = ACTIONS.indexOf(code.split(":")[1]!)
                        return index < 0 ? 100 : index
                    }
                    return rank(a) - rank(b)
                }),
        }))
        .filter((entry) => entry.codes.length)
        .sort((a, b) => {
            const rank = (resource: string) => {
                const index = PRIMARY.indexOf(resource)
                return index < 0 ? 100 : index
            }
            return rank(a.resource) - rank(b.resource)
        })
}

/** Expand only this resource's broad grant, preserving other resources and unknown explicit grants. */
export function selectResourceActions(
    permissions: readonly string[],
    resource: string,
    codes: readonly string[],
) {
    if (!permissions.includes(`${resource}:*`)) return [...permissions]
    return [
        ...new Set([
            ...permissions.filter((code) => code !== `${resource}:*`),
            ...codes,
        ]),
    ]
}

export function permissionChanges(
    before: readonly string[],
    after: readonly string[],
) {
    const codes = [
        ...new Set([...PERMISSION_BY_CODE.keys(), ...before, ...after]),
    ].filter((code) => !code.includes("*"))
    return codes
        .filter(
            (code) =>
                hasPermission(before, code) !== hasPermission(after, code),
        )
        .map((code) => ({
            code,
            label: `${resourceLabel(code.split(":")[0]!)} · ${actionLabel(code.split(":")[1]!)}`,
            allowed: hasPermission(after, code),
        }))
}

/** Inert collaboration rules remain inspectable but must not obscure the effective business scope. */
export function meaningfulScope(row: DataScopeRecord) {
    return (
        row.enabled &&
        !(
            row.scopeType === "collaborative" &&
            ["customer", "contract", "sales_order"].includes(row.resource)
        )
    )
}
