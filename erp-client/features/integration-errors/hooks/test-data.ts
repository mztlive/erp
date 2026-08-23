/**
 * 测试夹具工厂：构造最小的 W29 队列视图/事项/处理结果对象。
 * 仅供本目录测试使用，不进入生产代码。
 */

import type {
    IntegrationFormalResult,
    IntegrationQueueView,
    IntegrationResolutionItemView,
} from "../types"

const TASK_ACTIONS = [
    "QUERY_ORIGINAL_RESULT",
    "REPLAY_ORIGINAL",
    "REATTRIBUTE",
    "LINK_COMPENSATION",
    "ADD_EVIDENCE",
    "RESOLVE",
] as const

export function makeTask(
    overrides: Partial<IntegrationResolutionItemView> = {},
): IntegrationResolutionItemView {
    return {
        identity: {
            itemType: "ERROR_TASK",
            id: "task-1",
            number: "task-1",
            subjectHash: "v1",
        },
        workItem: {
            workItemId: "wi-1",
            workItemType: "INTEGRATION_RESULT_UNKNOWN",
            taskVersion: "5",
            status: "OPEN",
            processingState: "READY",
            subjectVersion: "3",
            ownerUser: { id: "user-1", displayName: "张三" },
            allowedActions: ["CLOSE", "PROCESS", "VIEW"],
        },
        businessObject: {
            objectType: "BUSINESS_OBJECT",
            objectId: "bo-1",
            title: "采购订单 #1",
        },
        classification: {
            code: "result_unknown",
            errorClass: "result-unknown",
            label: "结果未知",
            severity: "critical",
            severityLabel: "阻断",
        },
        environment: "production",
        environmentLabel: "生产",
        status: { code: "pending", label: "待处理" },
        fundsImpact: "NONE",
        fundsImpactLabel: "无资金影响",
        compensationOpen: false,
        ageLabel: "2h",
        ownerRole: "采购",
        ownerUser: "user-1",
        createdAt: "2026-08-14T00:00:00.000Z",
        hasWorkItem: true,
        attempts: [],
        objectVersion: "1",
        allowedActions: [...TASK_ACTIONS],
        actionBlockers: [],
        repairLinks: [],
        auditTrail: [],
        evidenceTimeline: [],
        linkedEvidence: [],
        freshness: { updatedAt: "2026-08-14T00:00:00.000Z" },
        ...overrides,
    }
}

export function makeQueueView(
    ...all: IntegrationResolutionItemView[]
): IntegrationQueueView {
    const items = [...all]
    return {
        items,
        metrics: {
            resultUnknown: 0,
            manualRequired: 0,
            securityFaults: 0,
            openDifferences: 0,
            longestAgeLabel: items[0]?.ageLabel ?? "—",
        },
        context: {
            queueContextId: "queue:W29:mine:all",
            filterSummary: "视图=我的任务 · 模式=全部 · 环境=生产",
            updatedAt: "2026-08-14T00:00:00.000Z",
        },
    }
}

export function makeFormalResult(
    overrides: Partial<IntegrationFormalResult> = {},
): IntegrationFormalResult {
    return {
        status: "succeeded",
        title: "已标记解决",
        description: "处理已完成，可进入下一项。",
        workItemStatus: "COMPLETED",
        stayOnItem: false,
        terminal: true,
        ...overrides,
    }
}
