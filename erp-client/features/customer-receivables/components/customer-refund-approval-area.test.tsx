import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { CustomerRefundApprovalArea } from "./customer-refund-approval-area"
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
import { mergeCustomerRefundAllowedActions } from "@/features/customer-receivables/lib/customer-refund-approval"

const recoveryState: { actions: RecoveryOption[] } = { actions: [] }

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-crf-1", actions: recoveryState.actions },
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
        id: "def-crf-1",
        name: "客户退款审批",
        version: 2,
        nodes: [
            { key: "n1", name: "退款复核", assigneeName: "张三" },
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
        id: "inst-crf-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "退款复核",
        currentAssigneeName: "张三",
        processName: "客户退款审批",
        processVersion: "2",
    },
    allowedActions: ["CANCEL"],
}

describe("CustomerRefundApprovalArea", () => {
    it("shows the bound route after create and does not offer a work item", () => {
        wrapper(
            <CustomerRefundApprovalArea
                phase="draft"
                approval={binding}
                documentId="crf-1"
            />,
        )
        expect(screen.getByText("客户退款审批 v2")).toBeTruthy()
        expect(screen.getByText("退款复核")).toBeTruthy()
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
            <CustomerRefundApprovalArea phase="confirm" approval={binding} />,
        )
        expect(screen.getByText("张三 → 李四")).toBeTruthy()
        expect(
            screen.getByText("任一层驳回后，将从张三开始下一轮审批。"),
        ).toBeTruthy()
    })

    it("embeds runtime summary and history without deriving the next approver", () => {
        wrapper(
            <CustomerRefundApprovalArea
                phase="runtime"
                approval={running}
                documentId="crf-1"
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
            <CustomerRefundApprovalArea
                phase="runtime"
                approval={{ ...running, allowedActions: [] }}
                documentId="crf-1"
                workItemId="wi-crf-1"
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
                mergeCustomerRefundAllowedActions(
                    ["CANCEL"],
                    ["APPROVE", "REASSIGN", "CLOSE"],
                ),
            ),
        ).toBe(false)
        expect(
            mergeCustomerRefundAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })

    it("keeps rejected history across rounds and increments the displayed round", () => {
        wrapper(
            <CustomerRefundApprovalArea
                phase="runtime"
                approval={{
                    ...running,
                    instance: {
                        ...running.instance!,
                        currentRoundNo: 2,
                        latestRejection: "退款金额与原回款不一致",
                    },
                    recentHistory: [
                        {
                            executionId: "exec-1",
                            roundNo: 1,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "退款复核",
                            result: "REJECTED",
                            decisionReason: "退款金额与原回款不一致",
                        },
                        {
                            executionId: "exec-2",
                            roundNo: 2,
                            executionNo: 1,
                            nodeKey: "n1",
                            nodeName: "退款复核",
                            result: "ACTIVE",
                        },
                    ],
                }}
                documentId="crf-1"
            />,
        )
        expect(screen.getByText("当前轮次：第 2 轮")).toBeTruthy()
        expect(screen.getByText("第 1 轮")).toBeTruthy()
        expect(screen.getByText("第 2 轮")).toBeTruthy()
        expect(screen.getAllByText("退款复核 · 已驳回")).toHaveLength(1)
        expect(screen.getAllByText("退款复核 · 办理中")).toHaveLength(1)
        expect(screen.getByText("退款金额与原回款不一致")).toBeTruthy()
    })

    it("shows upgrade only when the server whitelist includes UPGRADE_BINDING", () => {
        wrapper(
            <CustomerRefundApprovalArea
                phase="draft"
                approval={binding}
                documentId="crf-1"
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
            <CustomerRefundApprovalArea
                phase="runtime"
                approval={running}
                documentId="crf-1"
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
            <CustomerRefundApprovalArea
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
                documentId="crf-1"
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

describe("customer refund decision whitelist and idempotency", () => {
    it("only emits the contract decision fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-crf-1",
            decision: "APPROVE",
            expectedTaskVersion: "2",
            idempotencyKey: "k-dec-crf",
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
            "wi-crf-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-crf-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const changed = slotForIntent(
            retry,
            "decision",
            "wi-crf-1",
            decisionIntentFingerprint("REJECT", "退款金额与原回款不一致"),
        )
        expect(retry.key).toBe(first.key)
        expect(changed.key).not.toBe(first.key)
    })
})
