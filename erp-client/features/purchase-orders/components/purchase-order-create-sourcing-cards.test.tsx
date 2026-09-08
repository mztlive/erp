import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"
import { useState } from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"
import { PurchaseOrderCreateSourcingCards } from "./purchase-order-create-sourcing-cards"
import {
    buildDefaultSourcingLines,
    type SourcingSalesOrder,
} from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"

vi.mock("@/features/entity-selectors", () => ({
    WarehouseSearchCombobox: () => null,
}))
afterEach(cleanup)
const order: SourcingSalesOrder = {
    salesOrderId: "so",
    salesOrderNo: "XS1",
    customerName: "客户",
    workItemId: "w1",
    lines: [
        {
            salesOrderLineId: "l1",
            itemName: "礼盒",
            unit: "盒",
            quantityScale: 0,
            salesQuantity: "1",
            coveredQuantity: "0",
            remainingQuantity: "1",
            deliveryDeadline: "2026-09-15",
            salesAllocationLabel: "明细 1",
            options: [
                {
                    sourceType: "PURCHASE",
                    supplierId: "s1",
                    supplierName: "供应商",
                    basisId: "b1",
                    workItemId: "w1",
                    purchaseType: "PHYSICAL",
                    fulfillmentResponsibility: "SUPPLIER_DIRECT",
                    paymentTermCode: "NET30",
                    paymentTermLabel: "月结",
                    unitCostGross: "10",
                    inputTaxRate: "0.13",
                    maxCreateQuantity: "1",
                    expectedDeliveryDate: "2026-09-15",
                },
            ],
        },
    ],
}
function Harness() {
    const [result, setResult] = useState("")
    const form = useAppForm({
        defaultValues: {
            salesOrderId: order.salesOrderId,
            lines: buildDefaultSourcingLines(order),
        },
        validators: {
            onChange: ({ value }) => sourcingFormValidationError(order, value),
        },
    })
    useStore(form.store, (state) => state.values.lines)
    return (
        <>
            <PurchaseOrderCreateSourcingCards
                form={form as unknown as PurchaseOrderCreateFormApi}
                order={order}
                onAddSplit={vi.fn()}
                onRemoveSplit={vi.fn()}
            />
            <button
                onClick={async () => {
                    await form.validate("submit")
                    setResult(form.state.canSubmit ? "允许预览" : "禁止预览")
                }}
            >
                验证预览
            </button>
            <output>{result}</output>
        </>
    )
}

test("摘要不显示编辑控件；修正非法数量并收起后仍能通过正式表单校验", async () => {
    render(<Harness />)
    expect(screen.queryByRole("spinbutton")).toBeNull()
    expect(screen.getByText("供应商")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "调整方案" }))
    expect(screen.queryByRole("button", { name: "拆分给其他来源" })).toBeNull()
    const quantity = screen.getByRole("spinbutton", {
        name: "本次分配数量，礼盒",
    })
    fireEvent.change(quantity, { target: { value: "0.5" } })
    fireEvent.click(screen.getByRole("button", { name: "验证预览" }))
    await waitFor(() => expect(screen.getByText("禁止预览")).toBeTruthy())
    fireEvent.change(quantity, { target: { value: "1" } })
    await waitFor(() =>
        expect(
            (
                screen.getByRole("button", {
                    name: "收起调整",
                }) as HTMLButtonElement
            ).disabled,
        ).toBe(false),
    )
    fireEvent.click(screen.getByRole("button", { name: "收起调整" }))
    expect(screen.queryByRole("spinbutton")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "验证预览" }))
    await waitFor(() => expect(screen.getByText("允许预览")).toBeTruthy())
})
