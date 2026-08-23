import type {
    IntegrationFormalResult,
    IntegrationQueueView,
    IntegrationResolutionItemView,
} from "../../types"

export function makeItem(
    overrides: Partial<IntegrationResolutionItemView> = {},
): IntegrationResolutionItemView {
    return {
        identity: {
            itemType: "ERROR_TASK",
            id: "task-1",
            number: "ET-1",
            subjectHash: "h1",
        },
        workItem: {
            workItemId: "wi_1",
            workItemType: "INTEGRATION_RESULT_UNKNOWN",
            taskVersion: "5",
            status: "OPEN",
            processingState: "READY",
            subjectVersion: "v3",
            ownerUser: { id: "u1", displayName: "张三" },
            allowedActions: ["CLOSE"],
        },
        businessObject: {
            objectType: "purchase_order",
            objectId: "po_1",
            title: "采购订单 PO-1",
        },
        classification: {
            code: "W29-1",
            errorClass: "parameter-or-mapping",
            label: "参数/映射错误",
            severity: "medium",
            severityLabel: "中",
        },
        environment: "production",
        environmentLabel: "生产",
        status: { code: "MANUAL_REQUIRED", label: "待人工处理" },
        fundsImpact: "NONE",
        fundsImpactLabel: "无资金影响",
        compensationOpen: false,
        ageLabel: "1 天",
        ownerRole: "采购",
        createdAt: "2026-08-01T00:00:00Z",
        hasWorkItem: true,
        attempts: [],
        objectVersion: "v1",
        allowedActions: ["QUERY_ORIGINAL_RESULT", "RESOLVE"],
        actionBlockers: [],
        repairLinks: [],
        auditTrail: [],
        evidenceTimeline: [],
        linkedEvidence: [],
        freshness: { updatedAt: "2026-08-01T00:00:00Z" },
        ...overrides,
    }
}

export function makeQueueView(
    overrides: Partial<IntegrationQueueView> = {},
): IntegrationQueueView {
    return {
        items: [makeItem()],
        metrics: {
            resultUnknown: 0,
            manualRequired: 1,
            securityFaults: 0,
            openDifferences: 0,
            longestAgeLabel: "1 天",
        },
        context: {
            queueContextId: "queue:W29:mine:all",
            filterSummary: "视图=我的任务",
            updatedAt: "2026-08-01T00:00:00Z",
        },
        ...overrides,
    }
}

export function makeResult(
    overrides: Partial<IntegrationFormalResult> = {},
): IntegrationFormalResult {
    return {
        status: "succeeded",
        title: "已处理",
        description: "处理记录已追加",
        stayOnItem: true,
        terminal: false,
        ...overrides,
    }
}
