import type { CustomerScope } from "@/features/customers/types"

export const SCOPE_LABELS: Record<CustomerScope, string> = {
    mine: "我的客户",
    collaborating: "协作客户",
    assigned: "我参与的客户",
    all_authorized: "全部有权客户",
}

export const SCOPE_ORDER: readonly CustomerScope[] = [
    "mine",
    "collaborating",
    "all_authorized",
]

/** 无 `customer_scope:detail` 时隐藏全部有权客户口径。 */
export function visibleCustomerScopes(
    canReadAll: boolean,
): readonly CustomerScope[] {
    if (canReadAll) return SCOPE_ORDER
    return SCOPE_ORDER.filter((scope) => scope !== "all_authorized")
}

export function parseCustomerScope(
    value: string | null | undefined,
): CustomerScope {
    if (
        value === "collaborating" ||
        value === "all_authorized" ||
        value === "mine"
    ) {
        return value
    }
    return "mine"
}
