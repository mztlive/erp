import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCancelApprovalButton } from "./sales-order-cancel-approval-button"

const mocks = vi.hoisted(() => ({
    cancelApproval: vi.fn(),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { userid: "sales-1" },
    }),
}))

vi.mock("@/features/sales-orders/hooks/queries", () => ({
    useCancelSalesOrderApprovalMutation: () => ({
        isPending: false,
        mutateAsync: mocks.cancelApproval,
    }),
}))

vi.mock(
    "@/features/sales-orders/hooks/use-sales-order-detail-permissions",
    () => ({
        useSalesOrderDetailPermissions: () => ({
            accountQuery: { isPending: false, isError: false },
            granted: ["sales_order:cancel_approval"],
        }),
    }),
)

const order = {
    id: "so-1",
    documentNumber: "XS202608230001",
    ownerUserId: "sales-1",
    primaryStatus: {
        code: "in_approval",
        label: "审批中",
        tone: "warning",
    },
    approval: { allowedActions: ["CANCEL"] },
    lockVersion: 7,
    version: 6,
} as unknown as SalesOrderDetailView

beforeEach(() => {
    mocks.cancelApproval.mockReset()
})

afterEach(() => {
    cleanup()
})

describe("SalesOrderCancelApprovalButton", () => {
    it("shows only the result, required reason, and confirmation actions", async () => {
        mocks.cancelApproval.mockResolvedValue({})
        const onResult = vi.fn()

        render(
            <SalesOrderCancelApprovalButton
                order={order}
                onResult={onResult}
            />,
        )

        fireEvent.click(screen.getByRole("button", { name: "撤回审批" }))

        expect(screen.getByRole("heading", { name: "撤回审批" })).toBeTruthy()
        expect(screen.getByText("撤回后，销售单将回到草稿。")).toBeTruthy()
        expect(screen.queryByText("提交后锁定字段")).toBeNull()
        expect(screen.queryByText("本次动作产生的影响")).toBeNull()
        expect(screen.queryByText("审批实例作废")).toBeNull()

        const reason = screen.getByRole("textbox", { name: "撤回原因" })
        const confirm = screen.getByRole("button", { name: "确认撤回" })
        expect((confirm as HTMLButtonElement).disabled).toBe(true)

        fireEvent.change(reason, { target: { value: "  客户要求调整  " } })
        expect((confirm as HTMLButtonElement).disabled).toBe(false)
        fireEvent.click(confirm)

        await waitFor(() => {
            expect(mocks.cancelApproval).toHaveBeenCalledWith(
                expect.objectContaining({
                    salesOrderId: "so-1",
                    expectedVersion: 7,
                    reason: "客户要求调整",
                }),
            )
        })
        await waitFor(() => {
            expect(onResult).toHaveBeenCalledWith(
                expect.objectContaining({
                    status: "succeeded",
                    title: "审批已撤回",
                }),
            )
        })
    })
})
