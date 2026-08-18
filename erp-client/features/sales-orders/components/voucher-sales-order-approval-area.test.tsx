import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { VoucherSalesOrderApprovalArea } from "./voucher-sales-order-approval-area"
import { hasForbiddenWorkItemActions } from "@/features/approval-workflow/components/approval-action-bar"
import {
    buildDecisionRequest,
    DECISION_REQUEST_KEYS,
    requestKeysOf,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    decisionIntentFingerprint,
    slotForIntent,
} from "@/features/approval-workflow/idempotency"
import { mergeVoucherSalesOrderAllowedActions } from "@/features/sales-orders/lib/voucher-sales-order-approval"

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-1", actions: [] },
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
        id: "def-v1",
        name: "卡券销售审批",
        version: 3,
        nodes: [
            { key: "n1", name: "销售审核", assigneeName: "张三" },
            { key: "n2", name: "卡券运营", assigneeName: "李四" },
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
        id: "inst-v1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "卡券运营",
        currentAssigneeName: "李四",
        processName: "卡券销售审批",
        processVersion: "3",
    },
    allowedActions: ["CANCEL"],
}

describe("VoucherSalesOrderApprovalArea", () => {
    it("shows the bound route after create and does not offer a work item", () => {
        wrapper(
            <VoucherSalesOrderApprovalArea
                phase="draft"
                approval={binding}
                documentId="so-v1"
            />,
        )
        expect(screen.getByText("卡券销售审批 v3")).toBeTruthy()
        expect(screen.getByText("销售审核")).toBeTruthy()
        expect(screen.getByText("张三")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "换人" })).toBeNull()
        expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
    })

    it("prints the submit confirmation route and fixed reject explanation", () => {
        wrapper(
            <VoucherSalesOrderApprovalArea phase="confirm" approval={binding} />,
        )
        expect(screen.getByText("张三 → 李四")).toBeTruthy()
        expect(
            screen.getByText("任一层驳回后，将从张三开始下一轮审批。"),
        ).toBeTruthy()
    })

    it("embeds runtime summary and history without deriving the next approver", () => {
        wrapper(
            <VoucherSalesOrderApprovalArea
                phase="runtime"
                approval={running}
                documentId="so-v1"
            />,
        )
        expect(screen.getByText("审批状态：审批中")).toBeTruthy()
        expect(screen.getByText("当前轮次：第 1 轮")).toBeTruthy()
        expect(screen.getByText("当前审批人：李四")).toBeTruthy()
        expect(screen.getByText("暂无审批记录")).toBeTruthy()
        expect(screen.queryByText("下一审批人")).toBeNull()
        expect(screen.queryByText("责任团队")).toBeNull()
        expect(screen.queryByText("POOL")).toBeNull()
    })

    it("only shows decision entries from the server whitelist", () => {
        wrapper(
            <VoucherSalesOrderApprovalArea
                phase="runtime"
                approval={{ ...running, allowedActions: [] }}
                documentId="so-v1"
                workItemId="wi-v1"
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
                mergeVoucherSalesOrderAllowedActions(
                    ["CANCEL"],
                    ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM", "CLOSE"],
                ),
            ),
        ).toBe(false)
        expect(
            mergeVoucherSalesOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("voucher sales order decision whitelist and idempotency", () => {
    it("only emits the contract decision fields", () => {
        const request = buildDecisionRequest({
            workItemId: "wi-vso-1",
            decision: "APPROVE",
            expectedTaskVersion: "2",
            idempotencyKey: "k-dec-v",
        })
        expect(requestKeysOf(request)).toEqual(
            [...DECISION_REQUEST_KEYS].filter((key) => key !== "reason").sort(),
        )
        expect(request).not.toHaveProperty("next_node")
        expect(request).not.toHaveProperty("reviewed_by")
        expect(request).not.toHaveProperty("work_item_type")
    })

    it("keeps the same key for the same intent and rotates after a change", () => {
        vi.spyOn(crypto, "randomUUID")
            .mockReturnValueOnce("aaa")
            .mockReturnValueOnce("bbb")
        const first = slotForIntent(
            null,
            "decision",
            "wi-vso-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-vso-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const changed = slotForIntent(
            retry,
            "decision",
            "wi-vso-1",
            decisionIntentFingerprint("REJECT", "价格不符"),
        )
        expect(retry.key).toBe(first.key)
        expect(changed.key).not.toBe(first.key)
    })
})
