import { canOpenWorkItemHandler } from "@/features/workspace/lib/navigation-eligibility"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

export function canProcess(item: WorkspaceWorkItem): boolean {
    return canOpenWorkItemHandler(
        item.allowedActions,
        item.actionBlockers.some(
            (blocker) =>
                blocker.action === "PROCESS" ||
                blocker.action === "OPEN_DOCUMENT",
        ),
    )
}

export function canView(item: WorkspaceWorkItem): boolean {
    return (
        item.allowedActions.includes("VIEW") ||
        item.allowedActions.includes("OPEN_DOCUMENT")
    )
}

export function isBlockedWorkItem(item: WorkspaceWorkItem): boolean {
    return (
        item.processingState !== "READY" || item.approval?.status === "BLOCKED"
    )
}
