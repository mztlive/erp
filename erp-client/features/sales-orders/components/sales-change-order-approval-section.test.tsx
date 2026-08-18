import type { ReactElement } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { SalesChangeOrderApprovalSection } from "./sales-change-order-approval-section"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"

vi.mock("@/features/approval-workflow/queries", async () => {
    const actual = await vi.importActual<
        typeof import("@/features/approval-workflow/queries")
    >("@/features/approval-workflow/queries")
    return {
        ...actual,
        useRecoveryOptionsQuery: () => ({
            data: { instanceId: "inst-sc-1", actions: [] },
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

vi.mock("@/features/sales-orders/hooks/queries", () => ({
    useSubmitSalesChangeOrderMutation: () => ({
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
        id: "def-sc-1",
        name: "销售变更审批",
        version: 2,
        nodes: [
            { key: "n1", name: "履约影响确认", assigneeName: "张三" },
            { key: "n2", name: "财务复核", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

const draftChange: SalesChangeOrderSummary = {
    id: "sc-1",
    statusLabel: "草稿",
    statusTone: "neutral",
    statusCode: "DRAFT",
    version: 2,
    baseRevisionNo: 3,
    createdAt: "2026-08-14T00:00:00.000Z",
    impactPath: "procurement",
    approval: binding,
}

describe("SalesChangeOrderApprovalSection", () => {
    it("shows submit only when the server whitelist includes SUBMIT", () => {
        wrapper(
            <SalesChangeOrderApprovalSection
                salesOrderId="so-1"
                nature="physical_service"
                changeOrder={draftChange}
            />,
        )
        expect(screen.getByRole("button", { name: "提交改单" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
    })

    it("hides submit when the server does not authorize it", () => {
        wrapper(
            <SalesChangeOrderApprovalSection
                salesOrderId="so-1"
                nature="physical_service"
                changeOrder={{
                    ...draftChange,
                    statusCode: "IN_APPROVAL",
                    approval: {
                        ...binding,
                        instance: {
                            id: "inst-sc-1",
                            status: "RUNNING",
                            currentRoundNo: 1,
                            currentNodeName: "履约影响确认",
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
