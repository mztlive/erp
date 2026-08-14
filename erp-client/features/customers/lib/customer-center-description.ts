import { SCOPE_LABELS } from "@/features/customers/lib/filter-customers"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import type { CustomerScope } from "@/features/customers/types"

function statusSuffix(status: DirectoryStatus): string {
    if (status === "active") return ""
    return status === "disabled" ? " · 停用" : " · 全部状态"
}

/**
 * 目录结果区描述文案：空范围、筛选无结果与命中结果三种口径。
 */
export function describeCustomerDirectoryTable(args: {
    scope: CustomerScope
    status: DirectoryStatus
    q: string
    totalInScope: number
    itemsLength: number
}): string {
    const { scope, status, q, totalInScope, itemsLength } = args
    const trimmedQ = q.trim()
    if (totalInScope === 0 && !trimmedQ && status === "active") {
        return `${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`
    }
    if (itemsLength === 0) {
        return `当前筛选无结果：${SCOPE_LABELS[scope]}${statusSuffix(status)}${trimmedQ ? ` · “${trimmedQ}”` : ""}`
    }
    if (scope !== "mine" || status !== "active" || trimmedQ) {
        return `当前筛选：${SCOPE_LABELS[scope]}${statusSuffix(status)}${trimmedQ ? ` · “${trimmedQ}”` : ""}`
    }
    return `${SCOPE_LABELS[scope]}下的全部客户；本页用于选择客户并进入其详情。`
}
