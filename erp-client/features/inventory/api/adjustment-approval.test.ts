import { describe, expect, it, vi } from "vitest"

import {
    buildDecisionRequest,
    DECISION_REQUEST_KEYS,
    requestKeysOf,
} from "@/features/approval-workflow/types"
import {
    decisionIntentFingerprint,
    slotForIntent,
} from "@/features/approval-workflow/idempotency"
import {
    adjustmentStatusMap,
    isDraftAdjustmentStatus,
    isRuntimeAdjustmentStatus,
} from "@/features/inventory/api/display"
import { mapAdjustmentApproval } from "@/features/inventory/api/mappers"
import {
    buildAdjustmentSubmitRequest,
    readInstanceResponsibility,
} from "@/features/inventory/api/adjustment"

describe("adjustmentStatusMap", () => {
    it("converges the page to DRAFT / IN_APPROVAL / POSTED", () => {
        expect(adjustmentStatusMap("DRAFT")).toMatchObject({
            status: "DRAFT",
            statusLabel: "草稿",
        })
        expect(adjustmentStatusMap("IN_APPROVAL")).toMatchObject({
            status: "IN_APPROVAL",
            statusLabel: "审批中",
        })
        expect(adjustmentStatusMap("POSTED")).toMatchObject({
            status: "POSTED",
            statusLabel: "已过账",
        })
        expect(adjustmentStatusMap("PENDING_WAREHOUSE_REVIEW")).toMatchObject({
            status: "IN_APPROVAL",
            statusLabel: "审批中",
        })
        expect(adjustmentStatusMap("PENDING_FINANCE_REVIEW")).toMatchObject({
            status: "IN_APPROVAL",
            statusLabel: "审批中",
        })
        expect(adjustmentStatusMap("REJECTED")).toMatchObject({
            status: "DRAFT",
            statusLabel: "草稿",
        })
        expect(isDraftAdjustmentStatus("REJECTED")).toBe(true)
        expect(adjustmentStatusMap("SOMETHING_ELSE")).toMatchObject({
            status: "UNCONFIRMED",
            statusLabel: "状态未确认",
        })
        expect(isDraftAdjustmentStatus("SOMETHING_ELSE")).toBe(false)
        expect(isRuntimeAdjustmentStatus("SOMETHING_ELSE")).toBe(false)
        expect(isRuntimeAdjustmentStatus("IN_APPROVAL")).toBe(true)
    })
})

describe("readInstanceResponsibility", () => {
    it("reads only instance fields and does not fall back to the first definition node", () => {
        const view = mapAdjustmentApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-1",
                name: "库存调整审批",
                version: 3,
                nodes: [{ key: "n1", name: "仓储审核", assignee_name: "张三" }],
            },
            instance: {
                id: "inst-1",
                status: "RUNNING",
                current_round_no: 1,
                current_node: "财务审核",
                current_assignee: "李四",
            },
            recent_history: [],
            allowed_actions: ["CANCEL"],
        })
        expect(readInstanceResponsibility(view)).toEqual({
            nextResponsible: "李四",
            currentNodeLabel: "财务审核",
        })
        expect(
            readInstanceResponsibility({
                requirement: "PROCESS_REQUIRED",
                definition: {
                    id: "def-1",
                    name: "库存调整审批",
                    version: 3,
                    nodes: [
                        { key: "n1", name: "仓储审核", assigneeName: "张三" },
                    ],
                    publishedNodes: [],
                },
                recentHistory: [],
                historyHasMore: false,
                allowedActions: [],
            }),
        ).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("buildAdjustmentSubmitRequest", () => {
    it("only sends the document version and idempotency key", () => {
        const request = buildAdjustmentSubmitRequest({
            expectedVersion: 3,
            idempotencyKey: "k-1",
        })
        expect(requestKeysOf(request)).toEqual([
            "expected_version",
            "idempotency_key",
        ])
        expect(request).not.toHaveProperty("reviewed_by")
        expect(request).not.toHaveProperty("next_assignee")
        expect(request).toEqual({
            expected_version: 3,
            idempotency_key: "k-1",
        })
    })
})

describe("mapAdjustmentApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapAdjustmentApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-1",
                name: "库存调整审批",
                version: 3,
                nodes: [{ key: "n1", name: "仓储审核", assignee_name: "张三" }],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view.instance).toBeUndefined()
        expect(view.definition?.name).toBe("库存调整审批")
        expect(view.definition?.nodes[0]?.assigneeName).toBe("张三")
        expect(view.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })
})

describe("stock adjustment decision whitelist and idempotency", () => {
    it("only emits the contract decision fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-adj-1",
            decision: "APPROVE",
            expectedTaskVersion: "2",
            idempotencyKey: "k-dec",
        })
        expect(requestKeysOf(request)).toEqual(
            [...DECISION_REQUEST_KEYS].filter((key) => key !== "reason").sort(),
        )
        expect(request).not.toHaveProperty("next_node")
        expect(request).not.toHaveProperty("reviewed_by")
    })

    it("keeps the same key for the same intent and rotates after a change", () => {
        vi.spyOn(crypto, "randomUUID")
            .mockReturnValueOnce("aaa")
            .mockReturnValueOnce("bbb")
        const first = slotForIntent(
            null,
            "decision",
            "wi-adj-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-adj-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const changed = slotForIntent(
            retry,
            "decision",
            "wi-adj-1",
            decisionIntentFingerprint("REJECT", "数量不符"),
        )
        expect(retry.key).toBe(first.key)
        expect(changed.key).not.toBe(first.key)
    })
})
