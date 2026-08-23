import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { PurchaseChangeOrderApprovalSection } from "./purchase-change-order-approval-section"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { PurchaseChangeOrderSummary } from "@/features/purchase-orders/types"

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-pco-1", actions: [] },
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

vi.mock("@/features/purchase-orders/hooks/queries", () => ({
    useSubmitPurchaseChangeMutation: () => ({
        mutateAsync: vi.fn(),
        isPending: false,
    }),
}))

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
    allowedActions: ["SUBMIT"],
}

const draftChange: PurchaseChangeOrderSummary = {
    id: "pco-1",
    purchaseOrderId: "po-1",
    statusLabel: "草稿",
    statusTone: "neutral",
    statusCode: "DRAFT",
    version: 2,
    reason: "采购变更",
    baseRevisionId: "rev-3",
    createdAt: "2026-08-14T00:00:00.000Z",
    approval: binding,
}

describe("PurchaseChangeOrderApprovalSection", () => {
    it("shows submit only when the server whitelist includes SUBMIT", () => {
        wrapper(
            <PurchaseChangeOrderApprovalSection
                purchaseOrderId="po-1"
                changeOrder={draftChange}
            />,
        )
        expect(screen.getByRole("button", { name: "提交改单" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
    })

    it("hides submit when the server does not authorize it", () => {
        wrapper(
            <PurchaseChangeOrderApprovalSection
                purchaseOrderId="po-1"
                changeOrder={{
                    ...draftChange,
                    statusCode: "IN_APPROVAL",
                    approval: {
                        ...binding,
                        instance: {
                            id: "inst-pco-1",
                            status: "RUNNING",
                            currentRoundNo: 1,
                            currentNodeName: "仓配影响确认",
                            currentAssigneeName: "张三",
                        },
                        allowedActions: ["CANCEL"],
                    },
                }}
            />,
        )
        expect(screen.queryByRole("button", { name: "提交改单" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
        expect(screen.queryByText("下一审批人")).toBeNull()
    })
})
