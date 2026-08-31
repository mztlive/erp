import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import type { StockAdjustmentApprovalView } from "@/features/inventory/types"

import { AdjustmentApprovalArea } from "./adjustment-approval-area"

const cancelMutation = vi.hoisted(() => ({
    isPending: false,
    mutateAsync: vi.fn(),
}))

vi.mock("@/features/inventory/hooks/queries", () => ({
    useCancelStockAdjustmentApprovalMutation: () => cancelMutation,
}))

vi.mock("@/features/approval-workflow/queries", () => ({
    useApprovalHistoryInfiniteQuery: () => ({
        data: undefined,
        hasNextPage: false,
        isFetchingNextPage: false,
        fetchNextPage: vi.fn(),
    }),
    useRecoveryOptionsQuery: () => ({ data: undefined }),
}))

vi.mock("@/features/approval-workflow/components/approval-action-bar", () => ({
    ApprovalActionBar: ({
        allowedActions,
        hiddenActions = [],
    }: {
        allowedActions: readonly string[]
        hiddenActions?: readonly string[]
    }) =>
        allowedActions.includes("CANCEL") &&
        !hiddenActions.includes("CANCEL") ? (
            <button type="button">撤回审批</button>
        ) : (
            <div data-testid="generic-approval-actions" />
        ),
}))

const baseApproval: StockAdjustmentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    instance: {
        id: "instance-1",
        status: "RUNNING",
        currentRoundNo: 1,
        currentNodeName: "仓储负责人审批",
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["CANCEL"],
}

beforeEach(() => {
    cancelMutation.mutateAsync.mockReset()
})

afterEach(cleanup)

test("allowed_actions 被污染但没有撤回令牌时不展示任何撤回入口", () => {
    render(
        <AdjustmentApprovalArea
            phase="runtime"
            approval={baseApproval}
            documentId="adjustment-1"
        />,
    )

    expect(screen.queryByRole("button", { name: "撤回审批" })).toBeNull()
})

test("专用撤回只提交详情令牌并展示 409 冲突原因", async () => {
    const conflict = Object.assign(
        new Error("库存调整单版本已变化，请刷新后重试"),
        { status: 409 },
    )
    cancelMutation.mutateAsync.mockRejectedValue(conflict)
    const command = {
        expectedVersion: "9007199254740993",
        approvalProcessInstanceId: "instance-1",
        expectedSubjectVersion: "4294967295",
        expectedInstanceVersion: "9007199254740997",
        expectedExecutionVersion: "9007199254740999",
        expectedTaskVersion: "9007199254741001",
    } as const

    render(
        <AdjustmentApprovalArea
            phase="runtime"
            approval={{ ...baseApproval, cancelCommand: command }}
            documentId="adjustment-1"
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: "撤回审批" }))
    fireEvent.change(screen.getByRole("textbox", { name: /原因/ }), {
        target: { value: "需要修改数量" },
    })
    fireEvent.click(screen.getByRole("button", { name: "确认撤回" }))

    await waitFor(() =>
        expect(cancelMutation.mutateAsync).toHaveBeenCalledWith({
            stockAdjustmentId: "adjustment-1",
            command,
            reason: "需要修改数量",
            idempotencyKey: expect.stringMatching(
                /^approval:cancel:instance-1:/,
            ),
        }),
    )
    expect((await screen.findByRole("alert")).textContent).toContain(
        "库存调整单版本已变化，请刷新后重试",
    )
})
