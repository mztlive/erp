import { describe, expect, it } from "vitest"

import {
    assignBestSuppliers,
    commonSuppliersForSelected,
    pickBestSourcingOption,
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

describe("assignBestSuppliers", () => {
    it("fills every line with its best supplier and max quantity", () => {
        const current: SourcingLineInput[] = [
            {
                salesOrderLineId: "l-1",
                selected: false,
                quantity: "1",
                supplierId: "",
            },
            {
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "1",
                supplierId: "s-high",
            },
        ]
        const next = assignBestSuppliers(
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
                salesOrderLineId: "l-1",
                selected: true,
                quantity: "8",
                supplierId: "s-a",
            },
            {
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "10",
                supplierId: "s-low",
            },
        ])
    })

    it("leaves a line unchanged when it has no qualified supply", () => {
        const current: SourcingLineInput[] = [
            {
                salesOrderLineId: "l-empty",
                selected: false,
                quantity: "4",
                supplierId: "",
            },
        ]
        expect(
            assignBestSuppliers(
                order([line({ salesOrderLineId: "l-empty", options: [] })]),
                current,
            ),
        ).toEqual(current)
    })
})

describe("commonSuppliersForSelected", () => {
    it("unions suppliers from selected lines instead of requiring an intersection", () => {
        const current: SourcingLineInput[] = [
            {
                salesOrderLineId: "l-1",
                selected: true,
                quantity: "1",
                supplierId: "",
            },
            {
                salesOrderLineId: "l-2",
                selected: true,
                quantity: "1",
                supplierId: "",
            },
        ]
        const options = commonSuppliersForSelected(
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
})
