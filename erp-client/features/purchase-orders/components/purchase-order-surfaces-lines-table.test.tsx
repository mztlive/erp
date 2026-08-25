import { render, screen } from "@testing-library/react"
import { beforeAll, expect, test } from "vitest"

import { makePurchaseOrderCenter } from "@/features/purchase-orders/hooks/use-purchase-order-detail-fixtures"

import { LinesTable } from "./purchase-order-surfaces-lines-table"

beforeAll(() => {
    class ResizeObserverStub {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    globalThis.ResizeObserver = ResizeObserverStub
})

test("采购明细用 DataTable 渲染行项目", () => {
    const order = makePurchaseOrderCenter()
    render(<LinesTable order={order} costMasked={false} />)

    expect(document.querySelector('[data-slot="data-table"]')).not.toBeNull()
    expect(screen.getByText("项目")).toBeTruthy()
    expect(screen.getByText("示例商品")).toBeTruthy()
    expect(screen.getByText("商品/服务")).toBeTruthy()
    expect(screen.getByText("13.00%")).toBeTruthy()
    for (const name of ["数量", "含税单价", "税率", "交期", "行含税", "税额"]) {
        expect(
            screen
                .getByRole("columnheader", { name: new RegExp(`^${name}`) })
                .getAttribute("data-align"),
        ).toBe("end")
    }
})

test("成本掩码时金额显示为占位", () => {
    const order = makePurchaseOrderCenter()
    render(<LinesTable order={order} costMasked />)

    expect(screen.getAllByText("•••").length).toBeGreaterThan(0)
    expect(screen.queryByText("1,130.00")).toBeNull()
})
