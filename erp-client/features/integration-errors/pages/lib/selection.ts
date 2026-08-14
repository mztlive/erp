import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import type {
    IntegrationItemType,
    IntegrationResolutionItemView,
} from "../../types"

export type IntegrationDetailTarget = {
    itemType: IntegrationItemType
    id: string
} | null

export function selectQueueSelection(
    queueItems: IntegrationResolutionItemView[],
    currentTaskId: string | undefined,
    currentDifferenceId: string | undefined,
): IntegrationResolutionItemView | undefined {
    if (currentTaskId) {
        return (
            queueItems.find(
                (candidate) =>
                    candidate.identity.itemType === "ERROR_TASK" &&
                    candidate.identity.id === currentTaskId,
            ) ??
            queueItems.find(
                (candidate) => candidate.identity.id === currentTaskId,
            )
        )
    }
    if (currentDifferenceId) {
        return queueItems.find(
            (candidate) =>
                candidate.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
                candidate.identity.id === currentDifferenceId,
        )
    }
    return queueItems[0]
}

export function resolveDetailTarget(
    forcedTaskId: string | undefined,
    forcedDifferenceId: string | undefined,
    queueSelection: IntegrationResolutionItemView | undefined,
): IntegrationDetailTarget {
    if (forcedTaskId) {
        return { itemType: "ERROR_TASK", id: forcedTaskId }
    }
    if (forcedDifferenceId) {
        return { itemType: "RECONCILIATION_DIFFERENCE", id: forcedDifferenceId }
    }
    if (queueSelection) {
        return {
            itemType: queueSelection.identity.itemType,
            id: queueSelection.identity.id,
        }
    }
    return null
}

export function resolveDisplayItem(
    detailTarget: IntegrationDetailTarget,
    detailData: IntegrationResolutionItemView | null | undefined,
    queueSelection: IntegrationResolutionItemView | undefined,
): IntegrationResolutionItemView | undefined {
    if (
        detailTarget &&
        detailData?.identity.itemType === detailTarget.itemType &&
        detailData.identity.id === detailTarget.id
    ) {
        return detailData
    }
    return queueSelection
}

export type IntegrationItemPosition = {
    currentIndex: number
    queueIndex: number
    positionIndex: number
    positionTotal: number
}

export function derivePosition(
    item: IntegrationResolutionItemView | undefined,
    items: IntegrationResolutionItemView[],
    queueItems: IntegrationResolutionItemView[],
    focusMode: boolean,
): IntegrationItemPosition {
    const currentIndex = item
        ? Math.max(
              0,
              items.findIndex((i) => i.identity.id === item.identity.id),
          )
        : 0
    const queueIndex = item
        ? queueItems.findIndex((i) => i.identity.id === item.identity.id)
        : -1
    const positionIndex = focusMode
        ? queueIndex >= 0
            ? queueIndex + 1
            : 1
        : currentIndex + 1
    const positionTotal = focusMode
        ? queueIndex >= 0
            ? queueItems.length
            : 1
        : items.length
    return { currentIndex, queueIndex, positionIndex, positionTotal }
}

export function deriveResponsibilityStatus(
    item: IntegrationResolutionItemView | undefined,
    userId: string | undefined,
): ResponsibilityStatus {
    if (!item?.workItem) {
        return item?.identity.itemType === "RECONCILIATION_DIFFERENCE"
            ? "assigned_to_me"
            : "blocked"
    }
    const workItem = item.workItem
    if (workItem.status === "COMPLETED") return "completed"
    if (workItem.status === "CLOSED") return "closed"
    if (workItem.processingState === "APPROVAL_BLOCKED") return "blocked"
    if (workItem.assignmentMode === "POOL" && !workItem.ownerUser) {
        return "pool_available"
    }
    return workItem.ownerUser?.id === userId
        ? "assigned_to_me"
        : "assigned_to_other"
}
