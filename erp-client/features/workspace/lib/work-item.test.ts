import { describe, expect, it } from "vitest"

import type { WorkspaceWorkItem } from "@/features/workspace/types"
import {
    canProcess,
    canView,
    isBlockedWorkItem,
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
        priority: 3,
        createdAt: "",
        ownerRoleLabel: "销售",
        ownerOrganizationLabel: "华东区",
        ownerUserLabel: "张三",
        reasonLabel: "",
        impactSummary: "",
        allowedActions: ["PROCESS", "VIEW"],
        actionBlockers: [],
        destinationWorkspaceId: "W01",
        handlerKey: "business_exception",
        enteredAtLabel: "",
        dueAtLabel: "",
        dueBucket: "later",
        family: "exception",
        ...overrides,
    }
}

describe("responsiblePartyLabel", () => {
    it("prefers the named owner and never says 团队待处理", () => {
        expect(responsiblePartyLabel(itemFixture())).toBe("销售 · 张三")
        expect(
            responsiblePartyLabel(
                itemFixture({ ownerUserLabel: "处理人待确认" }),
            ),
        ).not.toContain("团队待处理")
    })
})

describe("processBlocker / canProcess / canView", () => {
    it("reads server blockers and view actions", () => {
        expect(
            processBlocker(
                itemFixture({
                    actionBlockers: [
                        {
                            action: "PROCESS",
                            code: "ACTION_BLOCKED",
                            message: "请先复核",
                        },
                    ],
                }),
            ),
        ).toBe("请先复核")
        expect(canProcess(itemFixture())).toBe(true)
        expect(canProcess(itemFixture({ allowedActions: ["VIEW"] }))).toBe(true)
        expect(
            canProcess(
                itemFixture({
                    allowedActions: ["START_PROCESSING" as never],
                }),
            ),
        ).toBe(false)
        expect(
            canView(itemFixture({ allowedActions: ["OPEN_DOCUMENT"] })),
        ).toBe(true)
    })
})

describe("isBlockedWorkItem", () => {
    it("uses processing state or instance status, not a normal pending look", () => {
        expect(
            isBlockedWorkItem(
                itemFixture({ processingState: "APPROVAL_BLOCKED" }),
            ),
        ).toBe(true)
        expect(isBlockedWorkItem(itemFixture())).toBe(false)
    })
})
