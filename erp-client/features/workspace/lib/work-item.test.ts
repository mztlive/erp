import { describe, expect, it } from "vitest"

import type { WorkspaceWorkItem } from "@/features/workspace/types"
import { responsibilityText } from "@/lib/ui-text"
import {
    canProcess,
    canView,
    processBlocker,
    responsiblePartyLabel,
} from "./work-item"

function itemFixture(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-1",
        taskVersion: "v1",
        workItemType: "BUSINESS_EXCEPTION",
        workItemTypeLabel: "业务异常",
        businessObjectType: "SALES",
        businessObjectId: "N-1",
        subjectVersion: "sv-1",
        stableNumber: "N-1",
        objectTitle: "销售单 N-1",
        counterpartyName: "客户A",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        assignmentMode: "DIRECT",
        priority: 3,
        createdAt: "",
        dueAt: "",
        ownerRoleLabel: "销售",
        ownerOrganizationLabel: "华东区",
        reasonLabel: "",
        impactSummary: "",
        allowedActions: ["PROCESS", "VIEW"],
        actionBlockers: [],
        destinationWorkspaceId: "W02",
        handlerKey: "w02.process",
        enteredAtLabel: "",
        dueAtLabel: "",
        dueBucket: "later",
        family: "exception",
        ...overrides,
    }
}

describe("responsiblePartyLabel", () => {
    it("prefers the named owner", () => {
        expect(
            responsiblePartyLabel(itemFixture({ ownerUserLabel: "张三" })),
        ).toBe("销售 · 张三")
    })

    it("marks pool assignments as team-pending", () => {
        expect(
            responsiblePartyLabel(
                itemFixture({
                    ownerUserLabel: undefined,
                    assignmentMode: "POOL",
                }),
            ),
        ).toBe(`销售 · ${responsibilityText.poolAvailable}`)
    })

    it("falls back to the owning organization", () => {
        expect(
            responsiblePartyLabel(itemFixture({ ownerUserLabel: undefined })),
        ).toBe("销售 · 华东区")
    })
})

describe("processBlocker", () => {
    it("returns the first PROCESS blocker message", () => {
        const item = itemFixture({
            actionBlockers: [
                {
                    action: "PROCESS",
                    code: "ACTION_BLOCKED",
                    message: "请先复核",
                },
                {
                    action: "PROCESS",
                    code: "ACTION_BLOCKED",
                    message: "第二条",
                },
            ],
        })
        expect(processBlocker(item)).toBe("请先复核")
    })

    it("returns undefined when nothing blocks PROCESS", () => {
        expect(processBlocker(itemFixture())).toBeUndefined()
    })
})

describe("canProcess", () => {
    it("allows PROCESS when unblocked", () => {
        expect(canProcess(itemFixture())).toBe(true)
    })

    it("allows START_PROCESSING as handler navigation", () => {
        expect(
            canProcess(
                itemFixture({ allowedActions: ["START_PROCESSING", "VIEW"] }),
            ),
        ).toBe(true)
    })

    it("denies PROCESS when a blocker exists", () => {
        const item = itemFixture({
            actionBlockers: [
                {
                    action: "PROCESS",
                    code: "ACTION_BLOCKED",
                    message: "请先复核",
                },
            ],
        })
        expect(canProcess(item)).toBe(false)
    })

    it("denies when only VIEW is allowed", () => {
        expect(canProcess(itemFixture({ allowedActions: ["VIEW"] }))).toBe(
            false,
        )
    })
})

describe("canView", () => {
    it("reflects the VIEW action", () => {
        expect(canView(itemFixture())).toBe(true)
        expect(canView(itemFixture({ allowedActions: ["PROCESS"] }))).toBe(
            false,
        )
    })
})
