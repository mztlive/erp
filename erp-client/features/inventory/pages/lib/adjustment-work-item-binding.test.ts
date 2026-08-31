import { describe, expect, it } from "vitest"

import type { WorkItemProjection } from "@/features/work-items/types"
import { bindAdjustmentDecisionWorkItem } from "./adjustment-work-item-binding"

const runtime = {
    id: "instance-1",
    status: "RUNNING",
    currentRoundNo: 1,
    subjectVersion: "1",
    currentExecutionId: "execution-1",
    currentTaskId: "work-item-1",
    currentTaskVersion: "7",
} as const

function workItem(
    overrides: Partial<WorkItemProjection> = {},
): WorkItemProjection {
    return {
        workItemId: "work-item-1",
        workItemType: "DOCUMENT_APPROVAL",
        handlerKey: "document_approval",
        approvalProcessInstanceId: "instance-1",
        approvalNodeExecutionId: "execution-1",
        status: "OPEN",
        assignmentSource: "SYSTEM_RULE",
        ownerRole: "warehouse_manager",
        ownerRoleLabel: "仓库负责人",
        ownerOrganization: { id: "warehouse-1", displayName: "一号仓" },
        processingState: "READY",
        businessObjectType: "stock_adjustment",
        businessObjectId: "adjustment-1",
        rootBusinessObjectId: "adjustment-1",
        businessObjectLabel: "ADJ-1",
        subjectVersion: "1",
        taskVersion: "7",
        allowedActions: ["APPROVE", "REJECT"],
        actionBlockers: [],
        priority: 1,
        reasonLabel: "待审批",
        impactSummary: "库存调整",
        nextActionHint: "审批",
        summarySections: [],
        briefLines: [],
        briefMoreCount: 0,
        listSummary: "",
        createdAt: 1,
        ...overrides,
    }
}

describe("bindAdjustmentDecisionWorkItem", () => {
    it("只为同一库存调整和当前实例下发完整令牌", () => {
        expect(
            bindAdjustmentDecisionWorkItem(workItem(), "adjustment-1", runtime),
        ).toEqual({
            workItemId: "work-item-1",
            expectedTaskVersion: "7",
            allowedActions: ["APPROVE", "REJECT"],
        })
    })

    it.each([
        ["URL 单据错绑", { businessObjectId: "adjustment-2" }],
        ["实例错绑", { approvalProcessInstanceId: "instance-2" }],
        ["非库存调整", { businessObjectType: "purchase_order" }],
        ["非审批任务", { workItemType: "FULFILLMENT_OPERATION" }],
        ["终态任务", { status: "CLOSED" as const }],
        ["受阻任务", { processingState: "APPROVAL_BLOCKED" as const }],
        ["缺执行身份", { approvalNodeExecutionId: undefined }],
        ["非法任务版本", { taskVersion: "0" }],
    ])("%s 时失败关闭", (_label, overrides) => {
        expect(
            bindAdjustmentDecisionWorkItem(
                workItem(overrides),
                "adjustment-1",
                runtime,
            ),
        ).toBeUndefined()
    })

    it.each([
        ["主题版本错绑", { subjectVersion: "2" }],
        ["执行 ID 错绑", { currentExecutionId: "execution-2" }],
        ["任务 ID 错绑", { currentTaskId: "work-item-2" }],
        ["任务版本错绑", { currentTaskVersion: "8" }],
    ])("%s 时失败关闭", (_label, runtimeOverride) => {
        expect(
            bindAdjustmentDecisionWorkItem(workItem(), "adjustment-1", {
                ...runtime,
                ...runtimeOverride,
            }),
        ).toBeUndefined()
    })

    it.each(["APPROVED", "CANCELLED", "BLOCKED", "UNKNOWN"])(
        "运行时状态 %s 不下发决定令牌",
        (status) => {
            expect(
                bindAdjustmentDecisionWorkItem(workItem(), "adjustment-1", {
                    ...runtime,
                    status,
                }),
            ).toBeUndefined()
        },
    )

    it("详情或当前实例尚未加载时不暴露任务动作", () => {
        expect(
            bindAdjustmentDecisionWorkItem(workItem(), null, runtime),
        ).toBeUndefined()
        expect(
            bindAdjustmentDecisionWorkItem(
                workItem(),
                "adjustment-1",
                undefined,
            ),
        ).toBeUndefined()
    })
})
