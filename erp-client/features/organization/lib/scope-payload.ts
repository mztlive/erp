import { PERMISSION_GROUPS } from "@/lib/permissions.generated"
import type { CreateDataScopeInput } from "@/features/organization/types"

const RESOURCE_ACTION_PATTERN = /^[a-z0-9_]{1,128}$/
const STABLE_ID_PATTERN = /^[A-Za-z0-9._-]{1,128}$/

/** 与 `erp-identity` `WIRED_CONSUMERS` 同步；未接线资源不得出现在可配置清单。 */
const WIRED_CONSUMERS: ReadonlyArray<{
    resource: string
    actions: readonly string[]
}> = [
    { resource: "org_unit", actions: ["list", "manage"] },
    {
        resource: "customer",
        actions: ["list", "detail", "create", "update", "delete"],
    },
    { resource: "contract", actions: ["list", "detail", "create", "update"] },
    {
        resource: "sales_order",
        actions: [
            "list",
            "detail",
            "create",
            "update",
            "delete",
            "submit",
            "cancel_approval",
        ],
    },
    {
        resource: "purchase_order",
        actions: [
            "list",
            "detail",
            "create",
            "update",
            "delete",
            "submit",
            "cancel_approval",
        ],
    },
    { resource: "cost_entry", actions: ["list", "detail"] },
    { resource: "cost_allocation", actions: ["list"] },
]

export function isRegisteredIdentifier(value: string): boolean {
    return RESOURCE_ACTION_PATTERN.test(value)
}

export function isStableIdentity(value: string): boolean {
    return STABLE_ID_PATTERN.test(value) && !value.includes("*")
}

export function isWiredResourceAction(
    resource: string,
    action: string,
): boolean {
    return WIRED_CONSUMERS.some(
        (item) => item.resource === resource && item.actions.includes(action),
    )
}

export function registeredResources(): Array<{
    resource: string
    actions: string[]
}> {
    const catalog = new Set<string>()
    for (const group of PERMISSION_GROUPS) {
        for (const item of group.permissions) {
            const resource = item.permission.resource
            const action = item.permission.action
            if (
                !isRegisteredIdentifier(resource) ||
                !isRegisteredIdentifier(action)
            ) {
                continue
            }
            catalog.add(`${resource}:${action}`)
        }
    }
    return WIRED_CONSUMERS.flatMap(({ resource, actions }) => {
        const wired = actions.filter(
            (action) =>
                isRegisteredIdentifier(action) &&
                catalog.has(`${resource}:${action}`),
        )
        return wired.length > 0
            ? [{ resource, actions: [...wired].sort() }]
            : []
    })
}

export function validateCreateDataScope(
    input: CreateDataScopeInput,
): string | null {
    if (!isRegisteredIdentifier(input.resource)) {
        return "资源必须使用已注册标识，禁止通配符和显示名"
    }
    if (
        input.actions.length === 0 ||
        input.actions.some((action) => !isRegisteredIdentifier(action))
    ) {
        return "动作必须使用已注册标识，禁止通配符和显示名"
    }
    if (
        input.actions.some(
            (action) => !isWiredResourceAction(input.resource, action),
        )
    ) {
        return "资源或动作尚未接入 DataScope v2"
    }
    if (!isStableIdentity(input.subjectId)) {
        return "主体必须使用稳定 ID，不能用显示名"
    }
    const needsTargets =
        input.scopeType === "organization" || input.scopeType === "team"
    if (!needsTargets) {
        if (
            input.targetMode != null ||
            input.includeDescendants != null ||
            input.scopeTargets.length > 0
        ) {
            return "公司、本人及协作范围不得携带组织目标"
        }
        return null
    }
    if (!input.targetMode) return "组织范围必须指定目标模式"
    if (
        input.targetMode !== "explicit" &&
        input.targetDimension !== "internal_org"
    ) {
        return "动态范围仅支持内部组织"
    }
    if (input.targetMode === "explicit") {
        if (input.scopeTargets.length === 0) return "显式模式必须指定目标"
        if (input.scopeTargets.some((id) => !isStableIdentity(id))) {
            return "目标必须使用稳定 ID，禁止通配符和显示名"
        }
    } else if (input.scopeTargets.length > 0) {
        return "动态模式不得混入静态目标"
    }
    const needsDescendants =
        input.targetDimension === "internal_org" &&
        input.targetMode !== "managed_orgs"
    if (needsDescendants === (input.includeDescendants == null)) {
        return "内部组织显式或本人组织模式必须明确是否包含下级"
    }
    return null
}

export function createDataScopePayload(input: CreateDataScopeInput) {
    const error = validateCreateDataScope(input)
    if (error) throw new Error(error)
    return input
}
