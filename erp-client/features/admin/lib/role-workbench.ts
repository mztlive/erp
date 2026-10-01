import {
    PERMISSION_BY_CODE,
    actionLabel,
    resourceLabel,
} from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"

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
