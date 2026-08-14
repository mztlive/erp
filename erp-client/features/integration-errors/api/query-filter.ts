/**
 * W29 队列视图的客户端筛选规则。
 * 从 mappers.ts 拆出；mappers.ts 统一再导出 matchesQuery。
 */

import type {
    IntegrationResolutionItemView,
    IntegrationResolutionQuery,
} from "../types"

export function matchesQuery(
    item: IntegrationResolutionItemView,
    q: IntegrationResolutionQuery,
): boolean {
    if (q.mode === "errors" && item.identity.itemType !== "ERROR_TASK") {
        return false
    }
    if (q.view === "result_unknown") {
        if (item.classification.errorClass !== "result-unknown") return false
    }
    if (q.view === "security") {
        if (item.classification.errorClass !== "authentication-or-signature")
            return false
    }
    if (q.view === "reconciliation") {
        if (item.identity.itemType !== "RECONCILIATION_DIFFERENCE") return false
    }
    if (q.view === "auto_retry") {
        if (
            item.classification.errorClass !== "network-timeout" &&
            item.classification.errorClass !== "rate-limited"
        ) {
            return false
        }
    }
    if (q.view === "resolved") {
        // open queue excludes resolved — detail path still works
        if (
            item.status.code !== "resolved" &&
            item.status.code !== "closed" &&
            !item.status.code?.startsWith("confirm")
        ) {
            return false
        }
    }
    if (q.errorClass && item.classification.errorClass !== q.errorClass) {
        return false
    }
    if (q.q) {
        const needle = q.q.toLowerCase()
        const hay = [
            item.identity.number,
            item.identity.id,
            item.businessObject.title,
            item.businessObject.objectId,
            item.classification.label,
        ]
            .join(" ")
            .toLowerCase()
        if (!hay.includes(needle)) return false
    }
    return true
}
