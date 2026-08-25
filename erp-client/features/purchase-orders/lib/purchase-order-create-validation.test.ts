import { describe, expect, it } from "vitest"

import type {
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./purchase-order-create-model"
import {
    collectSourcingErrorMessages,
    sourcingFormValidationError,
} from "./purchase-order-create-validation"

function option(
    overrides: Partial<SourcingSupplierOption> = {},
): SourcingSupplierOption {
    return {
        supplierId: "s-1",
        supplierName: "示例供应商",
        basisId: "basis-1",
        workItemId: "wi-1",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        unitCostGross: "10.00",
        inputTaxRate: "0.13",
        maxCreateQuantity: "10",
        expectedDeliveryDate: "2026-09-01",
        ...overrides,
    }
}

function order(lines: readonly SourcingProductLine[]): SourcingSalesOrder {
    return {
        salesOrderId: "so-1",
        salesOrderNo: "SO-1",
        customerName: "示例客户",
        workItemId: "wi-1",
        lines,
    }
}

const sampleLine: SourcingProductLine = {
    salesOrderLineId: "l-1",
    itemName: "测试SKU",
    unit: "件",
    salesQuantity: "10",
    coveredQuantity: "0",
    remainingQuantity: "10",
    salesAllocationLabel: "销售明细 1",
    options: [option({ maxCreateQuantity: "8" })],
}

describe("sourcingFormValidationError", () => {
    it("maps quantity overflow onto the line quantity field", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    selected: true,
                    supplierId: "s-1",
                    quantity: "20",
                },
            ],
        })
        expect(error?.fields["lines[0].quantity"]).toBe(
            "测试SKU：本次采购数量不能超过 8",
        )
    })

    it("maps a missing supplier onto the line supplier field", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    selected: true,
                    supplierId: "",
                    quantity: "8",
                },
            ],
        })
        expect(error?.fields["lines[0].supplierId"]).toBe(
            "测试SKU：请选择供应商",
        )
    })

    it("returns a form-level lines error when nothing is selected", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    selected: false,
                    supplierId: "s-1",
                    quantity: "8",
                },
            ],
        })
        expect(error?.fields.lines).toBe("请至少选择一条本次采购明细")
    })

    it("maps missing quantity to a Chinese quantity error instead of a Zod type message", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    selected: true,
                    supplierId: "s-1",
                    quantity: undefined as unknown as string,
                },
            ],
        })
        expect(error?.fields["lines[0].quantity"]).toBe(
            "测试SKU：本次采购数量必须是大于 0、最多 6 位小数的数值",
        )
        expect(JSON.stringify(error)).not.toContain("Invalid input")
    })

    it("maps undefined supplier to a Chinese supplier error", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    selected: true,
                    supplierId: undefined as unknown as string,
                    quantity: "8",
                },
            ],
        })
        expect(error?.fields["lines[0].supplierId"]).toBe(
            "测试SKU：请选择供应商",
        )
        expect(JSON.stringify(error)).not.toContain("Invalid input")
    })

    it("returns undefined when selected lines are valid", () => {
        expect(
            sourcingFormValidationError(order([sampleLine]), {
                salesOrderId: "so-1",
                lines: [
                    {
                        salesOrderLineId: "l-1",
                        selected: true,
                        supplierId: "s-1",
                        quantity: "8",
                    },
                ],
            }),
        ).toBeUndefined()
    })
})

describe("collectSourcingErrorMessages", () => {
    it("deduplicates field and form messages", () => {
        expect(
            collectSourcingErrorMessages({
                form: {
                    errors: [
                        { fields: { lines: "请至少选择一条本次采购明细" } },
                    ],
                },
                fields: {
                    "lines[0].quantity": {
                        errors: ["测试SKU：本次采购数量不能超过 8"],
                    },
                    "lines[1].quantity": {
                        errors: ["测试SKU：本次采购数量不能超过 8"],
                    },
                },
            }),
        ).toEqual([
            "请至少选择一条本次采购明细",
            "测试SKU：本次采购数量不能超过 8",
        ])
    })
})
