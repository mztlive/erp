import type { WorkItemAllowedAction } from "@/features/work-items"

import type { QueueWorkItemView } from "../types"

let seq = 0

export function makeQueueItem(
    overrides: Partial<QueueWorkItemView> = {},
): QueueWorkItemView {
    seq += 1
    const n = seq
    return {
        workItemId: `wi-${n}`,
        id: `wi-${n}`,
        workItemType: "po_review",
        workItemTypeLabel: "采购单财务审核",
        family: "finance",
        handlerKey: "po_review",
        handlerKnown: true,
        destinationWorkspaceId: "W08",
        status: "OPEN",
        assignmentMode: "DIRECT",
        assignmentSource: "role",
        ownerRole: "finance",
        ownerRoleLabel: "财务",
        ownerOrganization: { id: "org-1", displayName: "财务部" },
        ownerUser: { id: `user-${n}`, displayName: `处理人${n}` },
        processingState: "READY",
        businessObjectType: "purchase_order",
        businessObjectId: `po-${n}`,
        rootBusinessObjectId: `po-${n}`,
        businessObject: `采购单 PO-${n}`,
        businessObjectLabel: `采购单 PO-${n}`,
        subjectVersion: "v1",
        taskVersion: `tv-${n}`,
        allowedActions: [
            "START_PROCESSING",
            "RELEASE_TO_TEAM",
            "REASSIGN",
            "CLOSE",
        ] as readonly WorkItemAllowedAction[],
        actionBlockers: [],
        priority: "normal",
        dueAt: 1_736_995_200,
        reasonLabel: "需要确认",
        reason: "需要确认",
        impact: "影响付款",
        impactSummary: "影响付款",
        nextActionHint: "进入对应页面后提交处理结论。",
        summarySections: [],
        createdAt: 1_736_899_200,
        enteredAt: "1月1日 08:00",
        enteredDateTime: "2026-01-01T00:00:00.000Z",
        dueLabel: "1月2日 08:00",
        dueDateTime: "2026-01-02T00:00:00.000Z",
        responsibilityLabel: `由 处理人${n} 处理`,
        statusPresentation: { label: "待处理", tone: "info" },
        priorityRank: 3,
        priorityLabel: "普通",
        ...overrides,
    }
}
