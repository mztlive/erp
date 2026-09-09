import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { WorkspacePaymentMergeDialog } from "./workspace-payment-merge-dialog"
import type { PaymentMergeTask } from "../lib/workspace-payment-merge"

afterEach(cleanup)

const items: readonly PaymentMergeTask[] = [
    {
        workItemId: "wi-1",
        taskVersion: "3",
        payableAccountId: "pa-1",
        subjectVersion: "8",
        sourceDocumentId: "po-1",
        sourceDocumentNo: "CG-1",
        openTotal: "10.00",
        isAnchor: true,
    },
    {
        workItemId: "wi-2",
        taskVersion: "1",
        payableAccountId: "pa-2",
        subjectVersion: "2",
        sourceDocumentId: "po-2",
        sourceDocumentNo: "CG-2",
        openTotal: "20.00",
        isAnchor: false,
    },
]

test("一键合并弹窗默认全选，取消附加任务后仍可确认", () => {
    const onConfirm = vi.fn()
    render(
        <WorkspacePaymentMergeDialog
            open
            items={items}
            supplierName="狮峰"
            onOpenChange={() => undefined}
            onConfirm={onConfirm}
        />,
    )

    expect(screen.getByText("合并付款")).toBeTruthy()
    expect(screen.getByText("CG-1 · 当前任务")).toBeTruthy()
    expect(screen.getByText("CG-2")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "开始合并付款" }))
    expect(onConfirm).toHaveBeenCalledTimes(1)
    expect([...onConfirm.mock.calls[0][0]].sort()).toEqual(["pa-1", "pa-2"])
})
