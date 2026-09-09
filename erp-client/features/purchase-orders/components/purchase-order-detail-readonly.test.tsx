import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"
import { makePurchaseOrderCenter } from "../hooks/use-purchase-order-detail-fixtures"
import { PurchaseOrderDetailPayableSection } from "./purchase-order-detail-payable-section"
import { PurchaseOrderDetailFulfillmentSection } from "./purchase-order-detail-fulfillment-section"

afterEach(cleanup)

test("票款摘要隐藏所有成本金额，财务权限不产生办理入口", () => {
    const order = makePurchaseOrderCenter({ allowedActions: ["PAY"] })
    const { container } = render(
        <PurchaseOrderDetailPayableSection order={order} costMasked />,
    )
    expect(container.textContent?.match(/•••/g)).toHaveLength(3)
    expect(container.textContent).not.toMatch(/1,130|0\.00/)
    expect(screen.queryByRole("link")).toBeNull()
    expect(
        screen
            .getAllByRole("button")
            .map((button) => button.getAttribute("aria-label")),
    ).toEqual(["付款核销说明", "收票核销说明"])
})

test("未形成应付保留缺省状态，不冒充零金额或空付款记录", () => {
    render(
        <PurchaseOrderDetailPayableSection
            order={makePurchaseOrderCenter({ payableSummary: undefined })}
            costMasked={false}
        />,
    )
    expect(screen.getByText("尚未形成应付")).toBeTruthy()
    expect(screen.queryByText(/¥/)).toBeNull()
    expect(screen.queryByRole("button")).toBeNull()
})

test("履约数量缺失时省略统计，先款缺口掩码且无跨岗位入口", () => {
    const base = makePurchaseOrderCenter()
    const order = makePurchaseOrderCenter({
        fulfillmentSummary: {
            ...base.fulfillmentSummary,
            inboundQty: "—",
            shippedQty: "—",
            remainingQty: "—",
        },
        allowedActions: ["PAY", "FULFILL"],
    })
    const gate = {
        ...base.progress.prepaymentGate,
        state: "BLOCKED" as const,
        required: "565.00",
        allocated: "100.00",
        gap: "465.00",
        message: "先款条件未满足",
    }
    const { container } = render(
        <PurchaseOrderDetailFulfillmentSection
            order={order}
            costMasked
            gate={gate}
        />,
    )
    expect(screen.queryByRole("heading", { name: "履约数量" })).toBeNull()
    expect(screen.getByText("未满足")).toBeTruthy()
    expect(container.textContent?.match(/•••/g)).toHaveLength(3)
    expect(container.textContent).not.toMatch(/565\.00|100\.00|465\.00/)
    expect(screen.queryByRole("link")).toBeNull()
    expect(screen.queryByRole("button")).toBeNull()
})
