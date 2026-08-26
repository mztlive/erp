import { describe, expect, it } from "vitest"

import {
    assignBestSourcingOptions,
    buildDefaultSourcingLines,
    buildSourcingWorkspace,
    commonSourcingOptionsForSelected,
    pickBestSourcingOption,
    sourcingFormLinesReady,
    summarizeSourcingOrder,
    type SourcingLineInput,
    type SourcingProductLine,
    type SourcingSalesOrder,
    type SourcingSupplierOption,
} from "./purchase-order-create-model"

function option(
    overrides: Partial<SourcingSupplierOption> &
        Pick<SourcingSupplierOption, "supplierId" | "supplierName">,
): SourcingSupplierOption {
    return {
        sourceType: "PURCHASE",
        basisId: `basis-${overrides.supplierId}`,
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

function line(
    overrides: Partial<SourcingProductLine> &
        Pick<SourcingProductLine, "salesOrderLineId" | "options">,
): SourcingProductLine {
    return {
        itemName: overrides.itemName ?? overrides.salesOrderLineId,
        unit: "件",
        salesQuantity: "10",
        coveredQuantity: "0",
        remainingQuantity: "10",
        deliveryDeadline: "2026-09-01",
        salesAllocationLabel: "销售明细 1",
        ...overrides,
    }
}

function order(lines: readonly SourcingProductLine[]): SourcingSalesOrder {
    return {
        salesOrderId: "so-1",
        salesOrderNo: "SO-1",
        customerName: "示例客户",
        contractNumber: "HT-1",
        salesOwnerName: "张三",
        workItemId: "wi-1",
        lines,
    }
}

describe("buildSourcingWorkspace", () => {
    it("keeps two fulfillment routes from the same supplier", () => {
        const shared = {
            sourceType: "PURCHASE" as const,
            workItemId: "wi-1",
            salesOrderId: "so-1",
            salesOrderNo: "SO-1",
            customerName: "示例客户",
            salesOrderRevisionId: "sor-1",
            supplierId: "s-1",
            supplierName: "示例供应商",
            purchaseType: "PHYSICAL" as const,
            paymentTermCode: "POSTPAY_NET30",
            paymentTermLabel: "货到 30 天",
            lines: [
                {
                    salesOrderLineId: "l-1",
                    salesOrderRevisionLineId: "sorl-1",
                    itemName: "商品",
                    salesQuantity: "10",
                    coveredQuantity: "0",
                    remainingQuantity: "10",
                    maxCreateQuantity: "10",
                    unit: "件",
                    unitCostGross: "8.00",
                    inputTaxRate: "0.13",
                    expectedDeliveryDate: "2026-09-01",
                    salesDeliveryDeadline: "2026-09-01",
                    salesAllocationLabel: "销售明细 1",
                },
            ],
            estimatedGross: "80.00",
            consumed: false,
        }
        const workspace = buildSourcingWorkspace([
            {
                ...shared,
                basisId: "basis-warehouse",
                fulfillmentResponsibility: "WAREHOUSE",
            },
            {
                ...shared,
                basisId: "basis-direct",
                fulfillmentResponsibility: "SUPPLIER_DIRECT",
            },
        ])

        expect(
            workspace[0]?.lines[0]?.options.map((item) => item.basisId),
        ).toEqual(["basis-direct", "basis-warehouse"])
    })
})

describe("pickBestSourcingOption", () => {
    it("picks the lowest gross cost", () => {
        const best = pickBestSourcingOption([
            option({
                supplierId: "s-high",
                supplierName: "高价",
                unitCostGross: "12.00",
            }),
            option({
                supplierId: "s-low",
                supplierName: "低价",
                unitCostGross: "9.50",
            }),
        ])
        expect(best?.supplierId).toBe("s-low")
    })

    it("breaks a cost tie with the earlier delivery date", () => {
        const best = pickBestSourcingOption([
            option({
                supplierId: "s-late",
                supplierName: "晚到",
                expectedDeliveryDate: "2026-10-01",
            }),
            option({
                supplierId: "s-early",
                supplierName: "早到",
                expectedDeliveryDate: "2026-09-10",
            }),
        ])
        expect(best?.supplierId).toBe("s-early")
    })

    it("prefers a supplier who can cover remaining quantity", () => {
        const best = pickBestSourcingOption(
            [
                option({
                    supplierId: "s-cheap-short",
                    supplierName: "便宜但不足",
                    unitCostGross: "8.00",
                    maxCreateQuantity: "2",
                }),
                option({
                    supplierId: "s-cover",
                    supplierName: "可覆盖",
                    unitCostGross: "9.00",
                    maxCreateQuantity: "10",
                }),
            ],
            "10",
        )
        expect(best?.supplierId).toBe("s-cover")
    })

    it("returns undefined when there is no option", () => {
        expect(pickBestSourcingOption([])).toBeUndefined()
    })

    it("still picks a supplier when cost scale exceeds the usual limit", () => {
        const best = pickBestSourcingOption([
            option({
                supplierId: "s-precise",
                supplierName: "高精度",
                unitCostGross: "9.12345",
            }),
            option({
                supplierId: "s-round",
                supplierName: "普通",
                unitCostGross: "10.00",
            }),
        ])
        expect(best?.supplierId).toBeTruthy()
    })
})

describe("assignBestSourcingOptions", () => {
    it("fills every line with its best supplier and max quantity", () => {
        const current: SourcingLineInput[] = [
            {
                rowKey: "l-1:0",
                salesOrderLineId: "l-1",
                selected: false,
                quantity: "1",
                basisId: "",
                expectedDeliveryDate: "2026-09-01",
            },
            {
                rowKey: "l-2:0",
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "1",
                basisId: "basis-s-high",
                expectedDeliveryDate: "2026-09-01",
            },
        ]
        const next = assignBestSourcingOptions(
            order([
                line({
                    salesOrderLineId: "l-1",
                    options: [
                        option({
                            supplierId: "s-a",
                            supplierName: "甲",
                            unitCostGross: "3.00",
                            maxCreateQuantity: "8",
                        }),
                    ],
                }),
                line({
                    salesOrderLineId: "l-2",
                    options: [
                        option({
                            supplierId: "s-high",
                            supplierName: "贵",
                            unitCostGross: "20.00",
                            maxCreateQuantity: "10",
                        }),
                        option({
                            supplierId: "s-low",
                            supplierName: "便宜",
                            unitCostGross: "11.00",
                            maxCreateQuantity: "10",
                        }),
                    ],
                }),
            ]),
            current,
        )
        expect(next).toEqual([
            {
                rowKey: "l-1:0",
                salesOrderLineId: "l-1",
                selected: true,
                quantity: "8",
                basisId: "basis-s-a",
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: "2026-09-01",
            },
            {
                rowKey: "l-2:0",
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "10",
                basisId: "basis-s-low",
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: "2026-09-01",
            },
        ])
    })

    it("leaves a line unchanged when it has no qualified supply", () => {
        const current: SourcingLineInput[] = [
            {
                rowKey: "l-empty:0",
                salesOrderLineId: "l-empty",
                selected: false,
                quantity: "4",
                basisId: "",
                expectedDeliveryDate: "2026-09-01",
            },
        ]
        expect(
            assignBestSourcingOptions(
                order([line({ salesOrderLineId: "l-empty", options: [] })]),
                current,
            ),
        ).toEqual(current)
    })
})

describe("commonSourcingOptionsForSelected", () => {
    it("unions suppliers from selected lines instead of requiring an intersection", () => {
        const current: SourcingLineInput[] = [
            {
                rowKey: "l-1:0",
                salesOrderLineId: "l-1",
                selected: true,
                quantity: "1",
                basisId: "",
                expectedDeliveryDate: "2026-09-01",
            },
            {
                rowKey: "l-2:0",
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "1",
                basisId: "",
                expectedDeliveryDate: "2026-09-01",
            },
        ]
        const options = commonSourcingOptionsForSelected(
            order([
                line({
                    salesOrderLineId: "l-1",
                    options: [
                        option({ supplierId: "s-a", supplierName: "甲" }),
                    ],
                }),
                line({
                    salesOrderLineId: "l-2",
                    options: [
                        option({ supplierId: "s-b", supplierName: "乙" }),
                    ],
                }),
            ]),
            current,
        )
        expect(options.map((item) => item.supplierId)).toEqual(["s-a", "s-b"])
    })
})

describe("summarizeSourcingOrder", () => {
    it("aggregates line, supplier and lowest-cost estimates", () => {
        const summary = summarizeSourcingOrder(
            order([
                line({
                    salesOrderLineId: "l-1",
                    coveredQuantity: "2",
                    remainingQuantity: "3",
                    options: [
                        option({
                            supplierId: "s-a",
                            supplierName: "甲",
                            unitCostGross: "10.00",
                            purchaseType: "PHYSICAL",
                        }),
                        option({
                            supplierId: "s-b",
                            supplierName: "乙",
                            unitCostGross: "12.00",
                            fulfillmentResponsibility: "SUPPLIER_DIRECT",
                            paymentTermLabel: "预付",
                        }),
                    ],
                }),
                line({
                    salesOrderLineId: "l-2",
                    remainingQuantity: "2",
                    options: [
                        option({
                            supplierId: "s-a",
                            supplierName: "甲",
                            unitCostGross: "4.50",
                            purchaseType: "VIRTUAL",
                        }),
                    ],
                }),
            ]),
        )
        expect(summary.lineCount).toBe(2)
        expect(summary.coveredLineCount).toBe(1)
        expect(summary.uniqueSupplierCount).toBe(2)
        expect(summary.purchaseTypes).toEqual(["PHYSICAL", "VIRTUAL"])
        expect(summary.fulfillmentResponsibilities).toEqual([
            "WAREHOUSE",
            "SUPPLIER_DIRECT",
        ])
        expect(summary.paymentTermLabels).toEqual(["货到 30 天", "预付"])
        expect(summary.businessCategories).toEqual([])
        expect(summary.minEstimatedGross).toBe("39.00")
    })

    it("estimates only the purchase residual after existing stock", () => {
        const summary = summarizeSourcingOrder(
            order([
                line({
                    salesOrderLineId: "l-1",
                    remainingQuantity: "10",
                    options: [
                        option({
                            sourceType: "EXISTING_STOCK",
                            supplierId: "",
                            supplierName: "现有库存 · 上海仓",
                            basisId: "stock-balance-1",
                            warehouseName: "上海仓",
                            sourceAvailableQuantity: "4",
                            maxCreateQuantity: "4",
                            unitCostGross: "0",
                            inputTaxRate: "0",
                        }),
                        option({
                            supplierId: "s-a",
                            supplierName: "甲",
                            unitCostGross: "10.00",
                            maxCreateQuantity: "10",
                        }),
                    ],
                }),
            ]),
        )

        expect(summary.uniqueSupplierCount).toBe(1)
        expect(summary.minEstimatedGross).toBe("60.00")
    })
})

describe("buildDefaultSourcingLines", () => {
    it("returns no form rows until a sales order is selected", () => {
        expect(buildDefaultSourcingLines(undefined)).toEqual([])
    })

    it("writes one selected row per remaining sales line", () => {
        expect(
            buildDefaultSourcingLines(
                order([
                    line({
                        salesOrderLineId: "l-1",
                        remainingQuantity: "1",
                        options: [
                            option({
                                supplierId: "s-a",
                                supplierName: "甲",
                                maxCreateQuantity: "1",
                            }),
                        ],
                    }),
                ]),
            ),
        ).toEqual([
            {
                rowKey: "l-1:0",
                salesOrderLineId: "l-1",
                selected: true,
                quantity: "1",
                basisId: "basis-s-a",
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: "2026-09-01",
            },
        ])
    })

    it("allocates existing stock first and assigns only the residual to purchase", () => {
        const rows = buildDefaultSourcingLines(
            order([
                line({
                    salesOrderLineId: "l-1",
                    remainingQuantity: "10",
                    options: [
                        option({
                            sourceType: "EXISTING_STOCK",
                            supplierId: "",
                            supplierName: "现有库存 · 上海仓",
                            basisId: "stock-balance-1",
                            warehouseName: "上海仓",
                            sourceAvailableQuantity: "4",
                            maxCreateQuantity: "4",
                            unitCostGross: "0",
                            inputTaxRate: "0",
                        }),
                        option({
                            supplierId: "s-a",
                            supplierName: "甲供应商",
                            maxCreateQuantity: "10",
                        }),
                    ],
                }),
            ]),
        )

        expect(rows.map((row) => [row.basisId, row.quantity])).toEqual([
            ["stock-balance-1", "4"],
            ["basis-s-a", "6"],
        ])
    })

    it("shares one stock balance capacity across multiple sales lines", () => {
        const stock = option({
            sourceType: "EXISTING_STOCK",
            supplierId: "",
            supplierName: "现有库存 · 上海仓",
            basisId: "stock-balance-1",
            warehouseName: "上海仓",
            sourceAvailableQuantity: "6",
            maxCreateQuantity: "5",
            unitCostGross: "0",
            inputTaxRate: "0",
        })
        const rows = buildDefaultSourcingLines(
            order([
                line({
                    salesOrderLineId: "l-1",
                    remainingQuantity: "5",
                    options: [stock],
                }),
                line({
                    salesOrderLineId: "l-2",
                    remainingQuantity: "5",
                    options: [
                        stock,
                        option({
                            supplierId: "s-a",
                            supplierName: "甲供应商",
                            maxCreateQuantity: "5",
                        }),
                    ],
                }),
            ]),
        )

        expect(
            rows.map((row) => [
                row.salesOrderLineId,
                row.basisId,
                row.quantity,
            ]),
        ).toEqual([
            ["l-1", "stock-balance-1", "5"],
            ["l-2", "stock-balance-1", "1"],
            ["l-2", "basis-s-a", "4"],
        ])
    })
})

describe("sourcingFormLinesReady", () => {
    it("treats empty form rows as not ready when the sales order has remaining lines", () => {
        expect(
            sourcingFormLinesReady(
                [],
                order([line({ salesOrderLineId: "l-1", options: [] })]),
            ),
        ).toBe(false)
    })

    it("is ready after default lines are written for the selected sales order", () => {
        const selected = order([
            line({
                salesOrderLineId: "l-1",
                options: [option({ supplierId: "s-a", supplierName: "甲" })],
            }),
        ])
        expect(
            sourcingFormLinesReady(
                buildDefaultSourcingLines(selected),
                selected,
            ),
        ).toBe(true)
    })
})
