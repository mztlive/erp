import type { ReactElement } from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import {
    ApprovalActionBar,
    hasForbiddenWorkItemActions,
} from "./approval-action-bar"
import { DefinitionBindingCard } from "./definition-binding-card"
import { ExecutionHistory, groupByRound } from "./execution-history"
import { RuntimeSummary } from "./runtime-summary"
import { SubmissionRouteConfirmation } from "./submission-route-confirmation"
import type { ApprovalDefinitionBinding, ApprovalHistoryItem } from "../types"

vi.mock("../queries", async (importOriginal) => {
    const actual = await importOriginal<typeof import("../queries")>()
    return {
        ...actual,
        useSubmitDecisionMutation: () => ({
            mutateAsync: vi.fn().mockResolvedValue({ instanceId: "inst-1" }),
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

const definition: ApprovalDefinitionBinding = {
    id: "def-1",
    name: "库存调整审批",
    version: 3,
    nodes: [
        { key: "n1", name: "销售审核", assigneeName: "张三" },
        { key: "n2", name: "财务审核", assigneeName: "李四" },
        { key: "n3", name: "运营审核", assigneeName: "王五" },
    ],
    publishedNodes: [],
}

describe("DefinitionBindingCard", () => {
    it("shows the bound route after create and does not offer editing", () => {
        render(<DefinitionBindingCard definition={definition} />)
        expect(screen.getByText("库存调整审批 v3")).toBeTruthy()
        expect(screen.getByText("销售审核")).toBeTruthy()
        expect(screen.getByText("张三")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "换人" })).toBeNull()
        expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
    })
})

describe("SubmissionRouteConfirmation", () => {
    it("prints the route and the fixed reject explanation", () => {
        render(<SubmissionRouteConfirmation definition={definition} />)
        expect(screen.getByText("张三 → 李四 → 王五")).toBeTruthy()
        expect(
            screen.getByText("任一层驳回后，将从张三开始下一轮审批。"),
        ).toBeTruthy()
    })
})

describe("RuntimeSummary", () => {
    it("uses a distinct blocked style instead of a normal pending look", () => {
        const { container } = render(
            <RuntimeSummary
                instance={{
                    id: "inst-1",
                    status: "BLOCKED",
                    currentRoundNo: 2,
                    currentNodeName: "销售审核",
                    currentAssigneeName: "张三",
                    latestRejection: "资料不全",
                    latestRejectionBy: "王五",
                    processName: "库存调整审批",
                    processVersion: "3",
                    blockerMessage: "审批人账号已停用",
                }}
            />,
        )
        expect(container.querySelector('[data-blocked="true"]')).toBeTruthy()
        expect(screen.getByText("审批状态：受阻")).toBeTruthy()
        expect(screen.getByText("当前轮次：第 2 轮")).toBeTruthy()
        expect(screen.getByText("最近驳回：王五 / 资料不全")).toBeTruthy()
    })
})

describe("ExecutionHistory", () => {
    it("keeps the same node across rounds as separate rows", () => {
        const items: ApprovalHistoryItem[] = [
            {
                executionId: "ex-1",
                roundNo: 1,
                executionNo: 1,
                nodeKey: "n1",
                nodeName: "销售审核",
                result: "REJECTED",
                decisionReason: "资料不全",
            },
            {
                executionId: "ex-2",
                roundNo: 2,
                executionNo: 1,
                nodeKey: "n1",
                nodeName: "销售审核",
                result: "ACTIVE",
            },
        ]
        render(<ExecutionHistory items={items} />)
        expect(screen.getByText("销售审核 · 已驳回")).toBeTruthy()
        expect(screen.getByText("销售审核 · 办理中")).toBeTruthy()
        expect(groupByRound(items)).toHaveLength(2)
    })
})

describe("ApprovalActionBar", () => {
    it("hides generic work-item actions on approval tasks", () => {
        expect(
            hasForbiddenWorkItemActions([
                "APPROVE",
                "REJECT",
                "START_PROCESSING",
            ]),
        ).toBe(true)
        wrapper(
            <ApprovalActionBar
                allowedActions={["APPROVE", "REJECT", "OPEN_DOCUMENT"]}
                workItemId="wi-1"
                expectedTaskVersion="3"
                documentHref="/inventory"
            />,
        )
        expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
        expect(screen.getByRole("button", { name: "驳回" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "开始处理" })).toBeNull()
        expect(screen.queryByRole("button", { name: "领取" })).toBeNull()
        expect(screen.queryByRole("button", { name: "退回团队" })).toBeNull()
        expect(screen.queryByRole("button", { name: "转交" })).toBeNull()
        expect(screen.queryByRole("button", { name: "关闭" })).toBeNull()
    })

    it("only shows personnel recovery when the server authorizes it", () => {
        wrapper(
            <ApprovalActionBar
                allowedActions={[
                    "RESUME_CURRENT_APPROVER",
                    "REASSIGN_CURRENT_APPROVER",
                ]}
                recoveryOptions={[
                    "RESUME_CURRENT_APPROVER",
                    "REASSIGN_CURRENT_APPROVER",
                ]}
                instance={{
                    id: "inst-1",
                    status: "BLOCKED",
                    currentRoundNo: 1,
                    instanceVersion: "2",
                    executionVersion: "3",
                    assignmentVersion: "1",
                }}
            />,
        )
        expect(
            screen.getByRole("button", { name: "恢复当前审批人" }),
        ).toBeTruthy()
        expect(
            screen.getByRole("button", { name: "改派当前审批人" }),
        ).toBeTruthy()
        expect(
            screen.queryByRole("button", { name: "取消受阻审批" }),
        ).toBeNull()
    })

    it("shows only blocked cancel for a non-personnel blocker", () => {
        wrapper(
            <ApprovalActionBar
                allowedActions={["CANCEL_BLOCKED_APPROVAL"]}
                recoveryOptions={["CANCEL_BLOCKED"]}
                instance={{
                    id: "inst-1",
                    status: "BLOCKED",
                    currentRoundNo: 1,
                    instanceVersion: "2",
                    executionVersion: "3",
                    blockerCode: "OPEN_TASK_CONFLICT",
                }}
            />,
        )
        expect(
            screen.getByRole("button", { name: "取消受阻审批" }),
        ).toBeTruthy()
        expect(
            screen.queryByRole("button", { name: "恢复当前审批人" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "改派当前审批人" }),
        ).toBeNull()
    })

    it("hides recovery on an active instance even if the page would like to infer it", () => {
        wrapper(
            <ApprovalActionBar
                allowedActions={["APPROVE"]}
                recoveryOptions={[]}
                workItemId="wi-1"
                expectedTaskVersion="1"
                instance={{
                    id: "inst-1",
                    status: "RUNNING",
                    currentRoundNo: 1,
                }}
            />,
        )
        expect(
            screen.queryByRole("button", { name: "改派当前审批人" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "恢复当前审批人" }),
        ).toBeNull()
    })

    it("submits approve without opening a dialog when asked", () => {
        wrapper(
            <ApprovalActionBar
                allowedActions={["APPROVE", "REJECT"]}
                workItemId="wi-1"
                expectedTaskVersion="3"
                approveWithoutDialog
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "通过" }))
        expect(screen.queryByRole("dialog")).toBeNull()
        expect(
            screen.queryByRole("button", { name: "确认通过" }),
        ).toBeNull()
        expect(screen.getByRole("button", { name: "驳回" })).toBeTruthy()
    })

    it("does not leak masked business fields when the viewer cannot read them", () => {
        wrapper(
            <ApprovalActionBar
                allowedActions={["VIEW"]}
                canReadSensitive={false}
            />,
        )
        expect(screen.getByText("当前账号无权查看部分业务字段")).toBeTruthy()
    })
})
