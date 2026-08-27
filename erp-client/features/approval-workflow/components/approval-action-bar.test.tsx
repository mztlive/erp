import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import { ApprovalActionBar } from "./approval-action-bar"

const submitDecision = vi.hoisted(() => ({
    isPending: false,
    mutateAsync: vi.fn(),
}))

vi.mock("../queries", () => ({
    useSubmitDecisionMutation: () => submitDecision,
}))

beforeEach(() => {
    submitDecision.mutateAsync.mockReset()
})

afterEach(cleanup)

test("通过动作先打开确认弹窗且不会直接提交", () => {
    render(
        <ApprovalActionBar
            allowedActions={["APPROVE", "REJECT"]}
            workItemId="work-item-1"
            expectedTaskVersion="3"
            decisionContext={{
                documentLabel: "采购单 CG202608270001",
                amountLabel: "¥1,398",
                currentNodeLabel: "采购负责人审批",
                impactSummary: "通过后进入财务审核。",
            }}
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: "通过" }))

    expect(screen.getByRole("dialog")).toBeTruthy()
    expect(screen.getByRole("button", { name: "确认通过" })).toBeTruthy()
    expect(screen.getByText("采购单 CG202608270001")).toBeTruthy()
    expect(screen.getByText("¥1,398")).toBeTruthy()
    expect(screen.getByText("采购负责人审批")).toBeTruthy()
    expect(screen.getByText("通过后进入财务审核。")).toBeTruthy()
    expect(submitDecision.mutateAsync).not.toHaveBeenCalled()
})

test("确认通过完成并关闭弹窗后才通知队列移除任务", async () => {
    const onDecisionApplied = vi.fn()
    const appliedView = { instance: { id: "instance-1" } }
    submitDecision.mutateAsync.mockResolvedValue(appliedView)
    render(
        <ApprovalActionBar
            allowedActions={["APPROVE"]}
            workItemId="work-item-1"
            expectedTaskVersion="3"
            onDecisionApplied={onDecisionApplied}
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: "通过" }))
    fireEvent.click(screen.getByRole("button", { name: "确认通过" }))

    await waitFor(() => expect(submitDecision.mutateAsync).toHaveBeenCalled())
    await waitFor(() =>
        expect(onDecisionApplied).toHaveBeenCalledWith(appliedView),
    )
    expect(screen.queryByRole("dialog")).toBeNull()
})
