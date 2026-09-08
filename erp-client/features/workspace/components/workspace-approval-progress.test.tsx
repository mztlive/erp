import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"
import { WorkspaceApprovalProgress } from "./workspace-approval-progress"

afterEach(cleanup)

test("受阻进度保留当前节点、审批人与驳回原因", () => {
    render(
        <WorkspaceApprovalProgress
            instance={{
                id: "a",
                status: "BLOCKED",
                currentRoundNo: 2,
                currentNodeName: "财务审核",
                currentAssigneeName: "张静",
                latestRejection: "请补充付款凭证",
                blockerMessage: "审批人已停用",
            }}
        />,
    )
    expect(
        document.querySelector('[aria-current="step"]')?.textContent,
    ).toContain("张静 · 受阻")
    expect(screen.getByText("请补充付款凭证")).toBeTruthy()
    expect(screen.getByText("审批人已停用")).toBeTruthy()
    expect(screen.getByText("第 2 轮")).toBeTruthy()
    expect(screen.queryByText("已通过")).toBeNull()
})

test("审批终态不继续标记处理中，撤回不显示通过", () => {
    const view = render(
        <WorkspaceApprovalProgress
            instance={{ id: "a", currentRoundNo: 1, status: "APPROVED" }}
        />,
    )
    expect(screen.getByText("已通过")).toBeTruthy()
    expect(document.querySelector('[aria-current="step"]')).toBeNull()
    view.rerender(
        <WorkspaceApprovalProgress
            instance={{ id: "a", currentRoundNo: 1, status: "CANCELLED" }}
        />,
    )
    expect(screen.getByText("已撤回")).toBeTruthy()
    expect(screen.queryByText("已通过")).toBeNull()
    expect(screen.queryByText("已完成")).toBeNull()
    expect(document.querySelector('[aria-current="step"]')).toBeNull()
})
