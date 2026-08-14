import type { ResponsibilityStatus } from "@/components/business"
import type { WorkItemAllowedAction } from "@/features/work-items"

import type { QueueScopeSlug, QueueWorkItemView } from "../types"

export function toResponsibilityStatus(
    item: QueueWorkItemView,
    scope: QueueScopeSlug,
    currentUserId?: string,
): ResponsibilityStatus {
    if (item.status === "COMPLETED") return "completed"
    if (item.status === "CLOSED") return "closed"
    if (item.processingState === "APPROVAL_BLOCKED") return "blocked"
    if (item.assignmentMode === "POOL" && item.ownerUser === undefined) {
        return "pool_available"
    }
    return scope === "mine" || item.ownerUser?.id === currentUserId
        ? "assigned_to_me"
        : "assigned_to_other"
}

export function containsAction(
    item: QueueWorkItemView,
    action: WorkItemAllowedAction,
): boolean {
    return item.allowedActions.includes(action)
}
