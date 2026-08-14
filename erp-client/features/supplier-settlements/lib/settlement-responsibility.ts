import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"

/**
 * 结算复核任务 → 责任状态（与审批待办责任合同一致）。
 * 纯函数：仅依据任务字段与当前用户判定，不发请求。
 */
function responsibilityOf(
    workItem: SettlementDetailView["workItem"],
    currentUserId?: string,
): ResponsibilityStatus {
    if (!workItem) return "blocked"
    if (workItem.status === "COMPLETED") return "completed"
    if (workItem.status === "CLOSED") return "closed"
    if (workItem.processingState === "APPROVAL_BLOCKED") return "blocked"
    if (workItem.assignmentMode === "POOL" && !workItem.ownerUser) {
        return "pool_available"
    }
    return workItem.ownerUser?.id === currentUserId
        ? "assigned_to_me"
        : "assigned_to_other"
}

export { responsibilityOf }
