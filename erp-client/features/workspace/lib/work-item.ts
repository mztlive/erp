import { canOpenWorkItemHandler } from "@/features/workspace/lib/navigation-eligibility"
import type { WorkspaceWorkItem } from "@/features/workspace/types"
import { responsibilityText } from "@/lib/ui-text"

/** 任务行的责任方展示：已指派人 > 团队待处理 > 责任组织。 */
export function responsiblePartyLabel(item: WorkspaceWorkItem): string {
    if (item.ownerUserLabel) {
        return `${item.ownerRoleLabel} · ${item.ownerUserLabel}`
    }
    if (item.assignmentMode === "POOL") {
        return `${item.ownerRoleLabel} · ${responsibilityText.poolAvailable}`
    }
    return `${item.ownerRoleLabel} · ${item.ownerOrganizationLabel}`
}

/** 首个阻止「处理」动作的提示文案；无则 undefined。 */
export function processBlocker(item: WorkspaceWorkItem): string | undefined {
    return item.actionBlockers.find((b) => b.action === "PROCESS")?.message
}

export function canProcess(item: WorkspaceWorkItem): boolean {
    return canOpenWorkItemHandler(
        item.allowedActions,
        item.actionBlockers.some((blocker) => blocker.action === "PROCESS"),
    )
}

export function canView(item: WorkspaceWorkItem): boolean {
    return item.allowedActions.includes("VIEW")
}
