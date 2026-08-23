import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { PurchaseChangeOrderApprovalArea } from "./purchase-change-order-approval-area"
import { hasForbiddenWorkItemActions } from "@/features/approval-workflow/components/approval-action-bar"
import {
    buildDecisionRequest,
    DECISION_REQUEST_KEYS,
    requestKeysOf,
    type DocumentApprovalView,
    type RecoveryOption,
} from "@/features/approval-workflow/types"
import {
    decisionIntentFingerprint,
    slotForIntent,
} from "@/features/approval-workflow/idempotency"
import { mergePurchaseChangeOrderAllowedActions } from "@/features/purchase-orders/lib/purchase-change-order-approval"

const recoveryState: { actions: RecoveryOption[] } = { actions: [] }

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-pco-1", actions: recoveryState.actions },
        }),
        useApprovalHistoryInfiniteQuery: () => ({
            data: undefined,
            hasNextPage: false,
            isFetchingNextPage: false,
            fetchNextPage: vi.fn(),
        }),
        useSubmitDecisionMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useUpgradeBindingMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useCancelApprovalMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useResumeApproverMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useCancelBlockedMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
    }
})

afterEach(() => {
    recoveryState.actions = []
    cleanup()
})

function wrapper(ui: ReactElement) {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false },
            mutations: { retry: false },
        },
    })
    return render(
        <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
    )
}

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-pco-1",
        name: "采购变更审批",
        version: 2,
        nodes: [
            { key: "n1", name: "仓配影响确认", assigneeName: "张三" },
            { key: "n2", name: "财务复核", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT", "UPGRADE_BINDING"],
}

const running: DocumentApprovalView = {
    ...binding,
    instance: {
        id: "inst-pco-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "仓配影响确认",
        currentAssigneeName: "张三",
        processName: "采购变更审批",
        processVersion: "2",
    },
    allowedActions: ["CANCEL"],
}

describe("PurchaseChangeOrderApprovalArea", () => {
    it("shows the bound route after create and does not offer a work item", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="draft"
                approval={binding}
                documentId="pco-1"
            />,
        )
        expect(screen.getByText("采购变更审批 v2")).toBeTruthy()
        expect(screen.getByText("仓配影响确认")).toBeTruthy()
        expect(screen.getByText("张三")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "换人" })).toBeNull()
        expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
        expect(screen.queryByRole("button", { name: "影响确认" })).toBeNull()
        expect(screen.queryByRole("button", { name: "财务复核" })).toBeNull()
        expect(screen.queryByText("待办")).toBeNull()
    })

    it("prints the submit confirmation route and fixed reject explanation", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="confirm"
                approval={binding}
            />,
        )
        expect(screen.getByText("张三 → 李四")).toBeTruthy()
        expect(
            screen.getByText("任一层驳回后，将从张三开始下一轮审批。"),
        ).toBeTruthy()
    })

    it("embeds runtime summary and history without deriving the next approver", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="runtime"
                approval={running}
                documentId="pco-1"
            />,
        )
        expect(screen.getByText("审批状态：审批中")).toBeTruthy()
        expect(screen.getByText("当前轮次：第 1 轮")).toBeTruthy()
        expect(screen.getByText("当前审批人：张三")).toBeTruthy()
        expect(screen.getByText("暂无审批记录")).toBeTruthy()
        expect(screen.queryByText("下一审批人")).toBeNull()
        expect(screen.queryByText("责任团队")).toBeNull()
        expect(screen.queryByText("POOL")).toBeNull()
        expect(screen.queryByText("warehouse-impact")).toBeNull()
        expect(screen.queryByText("finance-confirm")).toBeNull()
    })

    it("only shows decision entries from the server whitelist", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="runtime"
                approval={{ ...running, allowedActions: [] }}
                documentId="pco-1"
                workItemId="wi-pco-1"
                expectedTaskVersion="4"
                workItemAllowedActions={["APPROVE"]}
            />,
        )
        expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "驳回" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
        expect(screen.queryByRole("button", { name: "退回团队" })).toBeNull()
    })

    it("does not expose generic work item actions on an approval task", () => {
        expect(
            hasForbiddenWorkItemActions(
                mergePurchaseChangeOrderAllowedActions(
                    ["CANCEL"],
                    ["APPROVE", "REASSIGN", "CLOSE"],
                ),
            ),
        ).toBe(false)
        expect(
            mergePurchaseChangeOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })

    it("keeps rejected history across rounds and increments the displayed round", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="runtime"
                approval={{
                    ...running,
                    instance: {
                        ...running.instance!,
                        currentRoundNo: 2,
                        latestRejection: "交期不可接受",
                    },
                    recentHistory: [
                        {
                            executionId: "exec-1",
                            roundNo: 1,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "仓配影响确认",
                            result: "REJECTED",
                            decisionReason: "交期不可接受",
                        },
                        {
                            executionId: "exec-2",
                            roundNo: 2,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "仓配影响确认",
                            result: "ACTIVE",
                        },
                    ],
                }}
                documentId="pco-1"
            />,
        )
        expect(screen.getByText("当前轮次：第 2 轮")).toBeTruthy()
        expect(screen.getByText("第 1 轮")).toBeTruthy()
        expect(screen.getByText("第 2 轮")).toBeTruthy()
        expect(screen.getAllByText("仓配影响确认 · 已驳回")).toHaveLength(1)
        expect(screen.getAllByText("仓配影响确认 · 办理中")).toHaveLength(1)
        expect(screen.getByText("交期不可接受")).toBeTruthy()
    })

    it("shows upgrade only when the server whitelist includes UPGRADE_BINDING", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="draft"
                approval={binding}
                documentId="pco-1"
            />,
        )
        expect(
            screen.getByRole("button", { name: "更新审批流程版本" }),
        ).toBeTruthy()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "撤回审批" })).toBeNull()
    })

    it("shows withdraw only when CANCEL is authorized", () => {
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="runtime"
                approval={running}
                documentId="pco-1"
            />,
        )
        expect(screen.getByRole("button", { name: "撤回审批" })).toBeTruthy()
        expect(
            screen.queryByRole("button", { name: "恢复当前审批人" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "改派当前审批人" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "取消受阻审批" }),
        ).toBeNull()
    })

    it("shows blocked recovery only from recovery_options and allowed_actions", () => {
        recoveryState.actions = ["RESUME_CURRENT_APPROVER"]
        wrapper(
            <PurchaseChangeOrderApprovalArea
                phase="runtime"
                approval={{
                    ...running,
                    instance: {
                        ...running.instance!,
                        status: "BLOCKED",
                        blockerCode: "ASSIGNEE_UNAVAILABLE",
                    },
                    allowedActions: ["RESUME_CURRENT_APPROVER"],
                }}
                documentId="pco-1"
            />,
        )
        expect(screen.getByText("审批状态：受阻")).toBeTruthy()
        expect(
            screen.getByRole("button", { name: "恢复当前审批人" }),
        ).toBeTruthy()
        expect(
            screen.queryByRole("button", { name: "改派当前审批人" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "取消受阻审批" }),
        ).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
    })
})

describe("purchase change order decision whitelist and idempotency", () => {
    it("only emits the contract decision fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-pco-1",
            decision: "APPROVE",
            expectedTaskVersion: "2",
            idempotencyKey: "k-dec-pco",
        })
        expect(requestKeysOf(request)).toEqual(
            [...DECISION_REQUEST_KEYS].filter((key) => key !== "reason").sort(),
        )
        expect(request).not.toHaveProperty("next_node")
        expect(request).not.toHaveProperty("reviewed_by")
        expect(request).not.toHaveProperty("handler_key")
        expect(request).not.toHaveProperty("expected_subject_version")
    })

    it("keeps the same key for the same intent and rotates after a change", () => {
        vi.spyOn(crypto, "randomUUID")
            .mockReturnValueOnce("aaa")
            .mockReturnValueOnce("bbb")
        const first = slotForIntent(
            null,
            "decision",
            "wi-pco-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-pco-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const changed = slotForIntent(
            retry,
            "decision",
            "wi-pco-1",
            decisionIntentFingerprint("REJECT", "交期不可接受"),
        )
        expect(retry.key).toBe(first.key)
        expect(changed.key).not.toBe(first.key)
    })
})
