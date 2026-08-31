import { describe, expect, it } from "vitest"

import {
    closeAdjustmentPreviewPatch,
    openAdjustmentPreviewPatch,
} from "./adjustment-navigation"

describe("adjustment preview URL identity", () => {
    it("打开普通库存调整时清除旧 WorkItem 深链", () => {
        expect(openAdjustmentPreviewPatch("adjustment-1")).toEqual({
            view: "adjustment",
            adjustmentId: "adjustment-1",
            currentWorkItemId: null,
            workItemId: null,
            balanceId: null,
        })
    })

    it("关闭 WorkItem-only 深链时清除两类预览身份", () => {
        expect(closeAdjustmentPreviewPatch()).toEqual({
            adjustmentId: null,
            currentWorkItemId: null,
            workItemId: null,
        })
    })

    it.each([
        ["仅 current alias", { currentWorkItemId: "current-task" }],
        ["仅 work alias", { workItemId: "legacy-task" }],
        [
            "两种 alias 同存",
            {
                currentWorkItemId: "current-task",
                workItemId: "legacy-task",
            },
        ],
    ])("普通 open/close 都清除%s", (_label, existing) => {
        const opened = {
            ...existing,
            ...openAdjustmentPreviewPatch("adjustment-2"),
        }
        const closed = {
            ...existing,
            ...closeAdjustmentPreviewPatch(),
        }

        expect(opened.currentWorkItemId).toBeNull()
        expect(opened.workItemId).toBeNull()
        expect(closed.currentWorkItemId).toBeNull()
        expect(closed.workItemId).toBeNull()
    })
})
