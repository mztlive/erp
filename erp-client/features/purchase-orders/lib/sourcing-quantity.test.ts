import { expect, test } from "vitest"
import {
    canSplitSourcingProduct,
    sourcingQuantityStep,
    sourcingUnitQuantityError,
} from "./sourcing-quantity"
import { sourcingFormValidationError } from "./purchase-order-create-validation"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./purchase-order-create-model"

const option: SourcingSupplierOption = {
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
}
const product: SourcingProductLine = {
    salesOrderLineId: "l1",
    itemName: "礼盒",
    unit: "盒",
    quantityScale: 0,
    salesQuantity: "1",
    coveredQuantity: "0",
    remainingQuantity: "1",
    deliveryDeadline: "2026-09-15",
    salesAllocationLabel: "明细 1",
    options: [option, { ...option, basisId: "b2" }],
}

test("一整盒没有拆分入口，允许分数的单位与多盒需求可以拆分", () => {
    expect(canSplitSourcingProduct(product, 1)).toBe(false)
    expect(
        canSplitSourcingProduct({ ...product, remainingQuantity: "2" }, 1),
    ).toBe(true)
    expect(
        canSplitSourcingProduct(
            { ...product, quantityScale: 2, unit: "千克" },
            1,
        ),
    ).toBe(true)
    expect(
        canSplitSourcingProduct({ ...product, quantityScale: null }, 1),
    ).toBe(false)
    expect(
        canSplitSourcingProduct({ ...product, remainingQuantity: "2" }, 2),
    ).toBe(false)
})
test("整数、小数和缺失单位精度分别校验", () => {
    expect(sourcingQuantityStep(0)).toBe("1")
    expect(sourcingQuantityStep(3)).toBe("0.001")
    expect(sourcingUnitQuantityError("0.5", 0)).toContain("正整数")
    expect(sourcingUnitQuantityError("1.000000", 0)).toBeUndefined()
    expect(sourcingUnitQuantityError("0.25", 2)).toBeUndefined()
    expect(sourcingUnitQuantityError("0.001", 2)).toContain("2 位小数")
    expect(sourcingUnitQuantityError("1", undefined)).toContain("精度")
})
test("正式表单校验拒绝半盒，现有库存不要求采购交期", () => {
    const order: SourcingSalesOrder = {
        salesOrderId: "so1",
        salesOrderNo: "XS1",
        customerName: "客户",
        workItemId: "w1",
        lines: [product],
    }
    const row = {
        rowKey: "r1",
        salesOrderLineId: "l1",
        selected: true,
        basisId: "b1",
        quantity: "0.5",
        expectedDeliveryDate: "2026-09-15",
    }
    expect(
        sourcingFormValidationError(order, {
            salesOrderId: "so1",
            lines: [row],
        })?.fields["lines[0].quantity"],
    ).toContain("正整数")
    const stockOrder = {
        ...order,
        lines: [
            {
                ...product,
                options: [{ ...option, sourceType: "EXISTING_STOCK" as const }],
            },
        ],
    }
    expect(
        sourcingFormValidationError(stockOrder, {
            salesOrderId: "so1",
            lines: [{ ...row, quantity: "1", expectedDeliveryDate: "" }],
        }),
    ).toBeUndefined()
})
