import { describe, expect, it } from "vitest"

import { resolveAcceptanceTaskContext } from "@/features/sales-orders/lib/acceptance-workspace-fetch"

const TASK = {
    workItemId: "wi-42",
    workItemType: "CUSTOMER_ACCEPTANCE_REGISTRATION",
    handlerKey: "customer_acceptance_registration",
    destinationWorkspaceId: "W06",
    businessObjectType: "sales_order",
    businessObjectId: "so-7",
    status: "OPEN" as const,
    taskVersion: "3",
    allowedActions: ["PROCESS"] as const,
}

describe("resolveAcceptanceTaskContext", () => {
    it("从销售单直接进入时可以不带任务身份", () => {
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
            }),
        ).toEqual({ workItem: null, blocker: null })
    })

    it("工作台进入时收下匹配的任务主键和版本", () => {
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
                workItemId: "wi-42",
                workItem: TASK,
            }),
        ).toEqual({
            workItem: { id: "wi-42", expectedTaskVersion: 3 },
            blocker: null,
        })
    })

    it("任务身份与本单不一致时失败关闭", () => {
        const blocked = {
            workItem: null,
            blocker:
                "当前客户验收任务已变化或与本单不一致，请返回工作台刷新后再登记。",
        }
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
                workItemId: "wi-42",
            }),
        ).toEqual(blocked)
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-other",
                workItemId: "wi-42",
                workItem: TASK,
            }),
        ).toEqual(blocked)
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
                workItemId: "wi-42",
                workItem: { ...TASK, status: "COMPLETED" },
            }),
        ).toEqual(blocked)
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
                workItemId: "wi-42",
                workItem: { ...TASK, allowedActions: ["VIEW"] },
            }),
        ).toEqual(blocked)
        expect(
            resolveAcceptanceTaskContext({
                salesOrderId: "so-7",
                workItemId: "wi-99",
                workItem: TASK,
            }),
        ).toEqual(blocked)
    })
})
