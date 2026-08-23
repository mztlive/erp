import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import {
    AdjustmentApprovalArea,
    mergeAdjustmentAllowedActions,
} from "./adjustment-approval-area"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

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
        useCancelBlockedMutation: () => ({
            mutateAsync: vi.fn(),
            isPending: false,
        }),
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
        id: "def-1",
        name: "库存调整审批",
        version: 3,
        nodes: [
            { key: "n1", name: "仓储审核", assigneeName: "张三" },
            { key: "n2", name: "财务审核", assigneeName: "李四" },
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
        id: "inst-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "仓储审核",
        currentAssigneeName: "张三",
        processName: "库存调整审批",
        processVersion: "3",
    },
    allowedActions: ["CANCEL"],
}

describe("AdjustmentApprovalArea", () => {
    it("shows the bound route after create and does not offer editing", () => {
        wrapper(
            <AdjustmentApprovalArea
                phase="draft"
                approval={binding}
                documentId="adj-1"
            />,
        )
        expect(screen.getByText("库存调整审批 v3")).toBeTruthy()
        expect(screen.getByText("仓储审核")).toBeTruthy()
        expect(screen.getByText("张三")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "换人" })).toBeNull()
        expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
    })

    it("prints the submit confirmation route and fixed reject explanation", () => {
        wrapper(<AdjustmentApprovalArea phase="confirm" approval={binding} />)
        expect(screen.getByText("张三 → 李四")).toBeTruthy()
        expect(
            screen.getByText("任一层驳回后，将从张三开始下一轮审批。"),
        ).toBeTruthy()
    })

    it("embeds runtime summary and history without deriving the next approver", () => {
        wrapper(
            <AdjustmentApprovalArea
                phase="runtime"
                approval={running}
                documentId="adj-1"
            />,
        )
        expect(screen.getByText("审批状态：审批中")).toBeTruthy()
        expect(screen.getByText("当前轮次：第 1 轮")).toBeTruthy()
        expect(screen.getByText("当前审批人：张三")).toBeTruthy()
        expect(screen.getByText("暂无审批记录")).toBeTruthy()
        expect(screen.queryByText("仓储复核")).toBeNull()
        expect(screen.queryByText("财务确认")).toBeNull()
    })

    it("only shows decision entries from the server whitelist", () => {
        wrapper(
            <AdjustmentApprovalArea
                phase="runtime"
                approval={{ ...running, allowedActions: [] }}
                documentId="adj-1"
                workItemId="wi-1"
                expectedTaskVersion="4"
                workItemAllowedActions={["APPROVE"]}
            />,
        )
        expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "驳回" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
    })
})

describe("mergeAdjustmentAllowedActions", () => {
    it("unions server facts and drops unknown codes", () => {
        expect(
            mergeAdjustmentAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})
