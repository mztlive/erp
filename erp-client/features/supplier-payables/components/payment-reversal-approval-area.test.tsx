import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { PaymentReversalApprovalArea } from "./payment-reversal-approval-area"
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
import { mergePaymentReversalAllowedActions } from "@/features/supplier-payables/lib/payment-reversal-approval"
import {
    approvalConflictMessage,
    isApprovalConflict,
} from "@/features/approval-workflow/api"

const recoveryState: { actions: RecoveryOption[] } = { actions: [] }

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-pr-1", actions: recoveryState.actions },
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
        useReassignApproverMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useCancelBlockedMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
        useEligibleReassigneesQuery: () => ({ data: [] }),
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
        id: "def-pr-1",
        name: "付款冲正审批",
        version: 2,
        nodes: [
            { key: "n1", name: "冲正复核", assigneeName: "张三" },
            { key: "n2", name: "财务确认", assigneeName: "李四" },
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
        id: "inst-pr-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "冲正复核",
        currentAssigneeName: "张三",
        processName: "付款冲正审批",
        processVersion: "2",
    },
    allowedActions: ["CANCEL"],
}

describe("PaymentReversalApprovalArea", () => {
    it("shows the bound route after create and does not offer a work item", () => {
        wrapper(
            <PaymentReversalApprovalArea
                phase="draft"
                approval={binding}
                documentId="pr-1"
            />,
        )
        expect(screen.getByText("付款冲正审批 v2")).toBeTruthy()
        expect(screen.getByText("冲正复核")).toBeTruthy()
        expect(screen.getByText("张三")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "换人" })).toBeNull()
        expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
        expect(screen.queryByRole("button", { name: "待复核" })).toBeNull()
        expect(screen.queryByText("待办")).toBeNull()
    })

    it("prints the submit confirmation route and fixed reject explanation", () => {
        wrapper(
            <PaymentReversalApprovalArea
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
            <PaymentReversalApprovalArea
                phase="runtime"
                approval={running}
                documentId="pr-1"
            />,
        )
        expect(screen.getByText("审批状态：审批中")).toBeTruthy()
        expect(screen.getByText("当前轮次：第 1 轮")).toBeTruthy()
        expect(screen.getByText("当前审批人：张三")).toBeTruthy()
        expect(screen.getByText("暂无审批记录")).toBeTruthy()
        expect(screen.queryByText("下一审批人")).toBeNull()
        expect(screen.queryByText("责任团队")).toBeNull()
        expect(screen.queryByText("POOL")).toBeNull()
        expect(screen.queryByText("pending_review")).toBeNull()
    })

    it("only shows decision entries from the server whitelist", () => {
        wrapper(
            <PaymentReversalApprovalArea
                phase="runtime"
                approval={{ ...running, allowedActions: [] }}
                documentId="pr-1"
                workItemId="wi-pr-1"
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
                mergePaymentReversalAllowedActions(
                    ["CANCEL"],
                    ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM", "CLOSE"],
                ),
            ),
        ).toBe(false)
        expect(
            mergePaymentReversalAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })

    it("keeps rejected history across rounds and increments the displayed round", () => {
        wrapper(
            <PaymentReversalApprovalArea
                phase="runtime"
                approval={{
                    ...running,
                    instance: {
                        ...running.instance!,
                        currentRoundNo: 2,
                        latestRejection: "冲正金额与原付款不一致",
                    },
                    recentHistory: [
                        {
                            executionId: "exec-1",
                            roundNo: 1,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "冲正复核",
                            result: "REJECTED",
                            decisionReason: "冲正金额与原付款不一致",
                        },
                        {
                            executionId: "exec-2",
                            roundNo: 2,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "冲正复核",
                            result: "ACTIVE",
                        },
                    ],
                }}
                documentId="pr-1"
            />,
        )
        expect(screen.getByText("当前轮次：第 2 轮")).toBeTruthy()
        expect(screen.getByText("第 1 轮")).toBeTruthy()
        expect(screen.getByText("第 2 轮")).toBeTruthy()
        expect(screen.getAllByText("冲正复核 · 已驳回")).toHaveLength(1)
        expect(screen.getAllByText("冲正复核 · 办理中")).toHaveLength(1)
        expect(screen.getByText("冲正金额与原付款不一致")).toBeTruthy()
    })

    it("shows upgrade only when the server whitelist includes UPGRADE_BINDING", () => {
        wrapper(
            <PaymentReversalApprovalArea
                phase="draft"
                approval={binding}
                documentId="pr-1"
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
            <PaymentReversalApprovalArea
                phase="runtime"
                approval={running}
                documentId="pr-1"
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
            <PaymentReversalApprovalArea
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
                documentId="pr-1"
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

describe("payment reversal decision whitelist and idempotency", () => {
    it("only emits the contract decision fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-pr-1",
            decision: "APPROVE",
            expectedTaskVersion: "2",
            idempotencyKey: "k-dec-pr",
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
            "wi-pr-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-pr-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const changed = slotForIntent(
            retry,
            "decision",
            "wi-pr-1",
            decisionIntentFingerprint("REJECT", "冲正金额与原付款不一致"),
        )
        expect(retry.key).toBe(first.key)
        expect(changed.key).not.toBe(first.key)
    })

    it("does not auto-replay a 409 conflict", () => {
        expect(isApprovalConflict({ status: 409 })).toBe(true)
        expect(approvalConflictMessage({ status: 409 })).toContain(
            "责任或版本已变化",
        )
        expect(isApprovalConflict({ status: 500 })).toBe(false)
    })
})
