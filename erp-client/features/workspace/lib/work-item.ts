import { canOpenWorkItemHandler } from "@/features/workspace/lib/navigation-eligibility"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/** 任务行的责任方展示。禁止写「团队待处理」。 */
export function responsiblePartyLabel(item: WorkspaceWorkItem): string {
    if (item.ownerUserLabel) {
        return `${item.ownerRoleLabel} · ${item.ownerUserLabel}`
    }
    return `${item.ownerRoleLabel} · ${item.ownerOrganizationLabel}`
}

/** 首个阻止打开单据的提示。 */
export function processBlocker(item: WorkspaceWorkItem): string | undefined {
    return item.actionBlockers.find(
        (blocker) =>
            blocker.action === "PROCESS" ||
            blocker.action === "OPEN_DOCUMENT" ||
            blocker.action === "APPROVE",
    )?.message
}

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
        item.processingState === "APPROVAL_BLOCKED" ||
        item.approval?.status === "BLOCKED"
    )
}
