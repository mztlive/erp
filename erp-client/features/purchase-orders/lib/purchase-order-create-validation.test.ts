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
        sourceType: "PURCHASE",
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
    deliveryDeadline: "2026-09-01",
    salesAllocationLabel: "销售明细 1",
    options: [option({ maxCreateQuantity: "8" })],
}

describe("sourcingFormValidationError", () => {
    it("maps quantity overflow onto the line quantity field", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: "20",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[0].quantity"]).toBe(
            "测试SKU：本次分配数量不能超过 8",
        )
    })

    it("maps a missing basis onto the line basis field", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "",
                    quantity: "8",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[0].basisId"]).toBe(
            "测试SKU：请选择履约方案",
        )
    })

    it("returns a form-level lines error when nothing is selected", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: false,
                    basisId: "basis-1",
                    quantity: "8",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields.lines).toBe("请至少选择一条本次供给分配明细")
    })

    it("maps missing quantity to a Chinese quantity error instead of a Zod type message", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: undefined as unknown as string,
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[0].quantity"]).toBe(
            "测试SKU：本次分配数量必须是大于 0、最多 6 位小数的数值",
        )
        expect(JSON.stringify(error)).not.toContain("Invalid input")
    })

    it("maps undefined basis to a Chinese basis error", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: undefined as unknown as string,
                    quantity: "8",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[0].basisId"]).toBe(
            "测试SKU：请选择履约方案",
        )
        expect(JSON.stringify(error)).not.toContain("Invalid input")
    })

    it("returns undefined when selected lines are valid", () => {
        expect(
            sourcingFormValidationError(order([sampleLine]), {
                salesOrderId: "so-1",
                lines: [
                    {
                        rowKey: "l-1:0",
                        salesOrderLineId: "l-1",
                        selected: true,
                        basisId: "basis-1",
                        quantity: "8",
                        expectedDeliveryDate: "2026-09-01",
                    },
                ],
            }),
        ).toBeUndefined()
    })

    it("rejects allocations that overdraw one shared stock balance", () => {
        const stock = option({
            sourceType: "EXISTING_STOCK",
            supplierId: "",
            supplierName: "现有库存 · 上海仓",
            basisId: "stock-balance-1",
            warehouseName: "上海仓",
            sourceAvailableQuantity: "10",
            maxCreateQuantity: "10",
        })
        const secondLine: SourcingProductLine = {
            ...sampleLine,
            salesOrderLineId: "l-2",
            itemName: "测试SKU二",
            salesAllocationLabel: "销售明细 2",
            options: [stock],
        }
        const error = sourcingFormValidationError(
            order([{ ...sampleLine, options: [stock] }, secondLine]),
            {
                salesOrderId: "so-1",
                lines: [
                    {
                        rowKey: "l-1:0",
                        salesOrderLineId: "l-1",
                        selected: true,
                        basisId: stock.basisId,
                        quantity: "6",
                        expectedDeliveryDate: "2026-09-01",
                    },
                    {
                        rowKey: "l-2:0",
                        salesOrderLineId: "l-2",
                        selected: true,
                        basisId: stock.basisId,
                        quantity: "6",
                        expectedDeliveryDate: "2026-09-01",
                    },
                ],
            },
        )
        expect(error?.fields["lines[1].quantity"]).toBe(
            "上海仓：库存分配合计不能超过 10",
        )
    })

    it("rejects an expected delivery date after the sales commitment", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: "8",
                    expectedDeliveryDate: "2026-09-02",
                },
            ],
        })
        expect(error?.fields["lines[0].expectedDeliveryDate"]).toBe(
            "测试SKU：预计交付日不能晚于销售承诺期限 2026-09-01",
        )
    })

    it("allows one sales line to split across different bases within remaining quantity", () => {
        const splitLine = {
            ...sampleLine,
            options: [
                option({ basisId: "basis-1", maxCreateQuantity: "10" }),
                option({
                    basisId: "basis-2",
                    fulfillmentResponsibility: "SUPPLIER_DIRECT",
                    maxCreateQuantity: "10",
                }),
            ],
        }
        expect(
            sourcingFormValidationError(order([splitLine]), {
                salesOrderId: "so-1",
                lines: [
                    {
                        rowKey: "l-1:0",
                        salesOrderLineId: "l-1",
                        selected: true,
                        basisId: "basis-1",
                        quantity: "4",
                        expectedDeliveryDate: "2026-09-01",
                    },
                    {
                        rowKey: "l-1:1",
                        salesOrderLineId: "l-1",
                        selected: true,
                        basisId: "basis-2",
                        quantity: "6",
                        expectedDeliveryDate: "2026-09-01",
                    },
                ],
            }),
        ).toBeUndefined()
    })

    it("rejects split quantities whose sum exceeds remaining quantity", () => {
        const splitLine = {
            ...sampleLine,
            options: [
                option({ basisId: "basis-1", maxCreateQuantity: "10" }),
                option({
                    basisId: "basis-2",
                    fulfillmentResponsibility: "SUPPLIER_DIRECT",
                    maxCreateQuantity: "10",
                }),
            ],
        }
        const error = sourcingFormValidationError(order([splitLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: "6",
                    expectedDeliveryDate: "2026-09-01",
                },
                {
                    rowKey: "l-1:1",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-2",
                    quantity: "5",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[1].quantity"]).toBe(
            "测试SKU：拆分数量合计不能超过 10",
        )
    })

    it("rejects the same basis twice for one sales line", () => {
        const error = sourcingFormValidationError(order([sampleLine]), {
            salesOrderId: "so-1",
            lines: [
                {
                    rowKey: "l-1:0",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: "4",
                    expectedDeliveryDate: "2026-09-01",
                },
                {
                    rowKey: "l-1:1",
                    salesOrderLineId: "l-1",
                    selected: true,
                    basisId: "basis-1",
                    quantity: "4",
                    expectedDeliveryDate: "2026-09-01",
                },
            ],
        })
        expect(error?.fields["lines[1].basisId"]).toBe(
            "测试SKU：同一履约方案不能重复选择",
        )
    })
})

describe("collectSourcingErrorMessages", () => {
    it("deduplicates field and form messages", () => {
        expect(
            collectSourcingErrorMessages({
                form: {
                    errors: [
                        {
                            fields: {
                                lines: "请至少选择一条本次供给分配明细",
                            },
                        },
                    ],
                },
                fields: {
                    "lines[0].quantity": {
                        errors: ["测试SKU：本次分配数量不能超过 8"],
                    },
                    "lines[1].quantity": {
                        errors: ["测试SKU：本次分配数量不能超过 8"],
                    },
                },
            }),
        ).toEqual([
            "请至少选择一条本次供给分配明细",
            "测试SKU：本次分配数量不能超过 8",
        ])
    })
})
