import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"
import { makePurchaseOrderCenter } from "@/features/purchase-orders/hooks/use-purchase-order-detail-fixtures"
import { PurchaseOrderDetailSidebar } from "./purchase-order-detail-sidebar"

afterEach(cleanup)

test("采购摘要沿用成本掩码，先款和票款金额也不暴露", () => {
    const base = makePurchaseOrderCenter()
    const order = makePurchaseOrderCenter({
        progress: {
            ...base.progress,
            prepaymentGate: {
                ...base.progress.prepaymentGate,
                state: "BLOCKED",
                required: "565.00",
                allocated: "100.00",
                gap: "465.00",
            },
        },
    })
    const { container } = render(
        <PurchaseOrderDetailSidebar order={order} costMasked />,
    )
    expect(screen.getAllByText("•••")).toHaveLength(9)
    expect(container.textContent).not.toMatch(
        /1,130|1,000|130\.00|565\.00|100\.00|465\.00/,
    )
    expect(screen.queryByRole("link", { name: "去供应商往来" })).toBeNull()
    expect(screen.queryByRole("link", { name: "去交付" })).toBeNull()
})

test("尚未形成应付时显示缺省提示，摘要不提供跨模块操作", () => {
    render(
        <PurchaseOrderDetailSidebar
            order={makePurchaseOrderCenter({ payableSummary: undefined })}
            costMasked={false}
        />,
    )
    expect(screen.getByText("尚未形成应付（需审批通过）。")).toBeTruthy()
    expect(screen.getAllByText("—")).toHaveLength(4)
    expect(screen.queryByRole("button")).toBeNull()
    for (const label of ["去交付", "去供应商往来", "去对账结算"]) {
        expect(screen.queryByText(label)).toBeNull()
    }
})
