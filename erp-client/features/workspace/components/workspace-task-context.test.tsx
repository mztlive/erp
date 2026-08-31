import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import type { WorkspaceWorkItem } from "../types"
import {
    WorkspaceTaskContextHelp,
    WorkspaceTaskHeaderActions,
    workspaceTaskHasInlineContextHelp,
} from "./workspace-task-context"

afterEach(cleanup)

function sampleItem(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-1",
        taskVersion: "1",
        workItemType: "DOCUMENT_APPROVAL",
        workItemTypeLabel: "销售单审批",
        businessObjectType: "sales_order",
        businessObjectId: "so-1",
        subjectVersion: "1",
        stableNumber: "XS20260831101354",
        objectTitle: "销售单 XS20260831101354",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        priority: 3,
        createdAt: "2026-08-31T10:13:00.000Z",
        ownerRole: "sales_order_approver",
        ownerRoleLabel: "销售单审批人",
        ownerOrganizationLabel: "责任组织",
        ownerUserLabel: "采购1",
        reasonLabel: "单据待审批",
        impactSummary: "不审批则销售单不能生效、不能履约",
        nextActionHint: "核对本页事实后，确认通过或驳回。",
        allowedActions: ["APPROVE", "REJECT"],
        actionBlockers: [],
        destinationWorkspaceId: "W01",
        handlerKey: "document_approval",
        enteredAtLabel: "8/31 10:13",
        dueAtLabel: "",
        dueBucket: "later",
        family: "approval",
        ...overrides,
    }
}

test("单据审批和供给分配在标题栏内嵌问号，未登记作业面才用右上角入口", () => {
    expect(
        workspaceTaskHasInlineContextHelp({
            workItemType: "DOCUMENT_APPROVAL",
        }),
    ).toBe(true)
    expect(
        workspaceTaskHasInlineContextHelp({
            workItemType: "PROCUREMENT_ORDER_CREATION",
        }),
    ).toBe(true)
    expect(
        workspaceTaskHasInlineContextHelp({
            workItemType: "CARD_FUNDS_REVIEW",
        }),
    ).toBe(false)
})

test("问号固定排在打开原单据动作后面", () => {
    render(
        <WorkspaceTaskHeaderActions item={sampleItem()}>
            <button type="button" aria-label="打开销售单">
                打开
            </button>
        </WorkspaceTaskHeaderActions>,
    )

    const buttons = screen.getAllByRole("button")
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([
        "打开销售单",
        "任务说明",
    ])
})

test("默认不铺开任务说明，点开问号后才展示到达原因与影响", () => {
    render(<WorkspaceTaskContextHelp item={sampleItem()} />)

    expect(screen.queryByText("为什么到你")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "任务说明" }))
    expect(screen.getByText("为什么到你")).toBeTruthy()
    expect(screen.getByText("单据待审批")).toBeTruthy()
    expect(screen.getByText("不审批则销售单不能生效、不能履约")).toBeTruthy()
    expect(screen.getByText("核对本页事实后，确认通过或驳回。")).toBeTruthy()
    expect(screen.getByText("销售单审批人 · 采购1 · 责任组织")).toBeTruthy()
    expect(screen.getByText("未设置截止时间")).toBeTruthy()
})

test("受阻任务在问号说明里列出阻塞原因", () => {
    render(
        <WorkspaceTaskContextHelp
            item={sampleItem({
                processingState: "APPROVAL_BLOCKED",
                actionBlockers: [
                    {
                        action: "APPROVE",
                        code: "BLOCKED",
                        message: "当前审批人已离职",
                    },
                ],
            })}
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: "任务说明" }))
    expect(screen.getByText("当前处理受阻")).toBeTruthy()
    expect(screen.getByText("当前审批人已离职")).toBeTruthy()
})
