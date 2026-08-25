import { describe, expect, it } from "vitest"

import type { PurchaseCreationBasis } from "@/features/purchase-orders/types"
import {
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildSourcingWorkspace,
    commonSuppliersForSelected,
    sourcingQuantityError,
} from "@/features/purchase-orders/lib/purchase-order-create-model"

function basis(
    input: Partial<PurchaseCreationBasis> &
        Pick<PurchaseCreationBasis, "basisId" | "supplierId" | "supplierName">,
): PurchaseCreationBasis {
    return {
        workItemId: "task-1",
        salesOrderId: "so-1",
        salesOrderNo: "SO-1",
        customerName: "客户甲",
        salesOrderRevisionId: "rev-1",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        estimatedGross: "10.00",
        consumed: false,
        lines: [
            {
                salesOrderLineId: "line-1",
                salesOrderRevisionLineId: "rline-1",
                itemName: "礼盒",
                itemSku: "SKU-1",
                salesQuantity: "2",
                coveredQuantity: "0",
                remainingQuantity: "2",
                maxCreateQuantity: "2",
                unit: "件",
                unitCostGross: "10.00",
                inputTaxRate: "0.13",
                expectedDeliveryDate: "2026-09-01",
                salesAllocationLabel: "销售明细 1",
            },
        ],
        ...input,
    }
}

describe("buildSourcingWorkspace", () => {
    it("把同一销售行的多家供应商收成一行选项", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
            }),
            basis({
                basisId: "b-b",
                supplierId: "sup-b",
                supplierName: "供应商B",
            }),
        ])
        expect(workspace).toHaveLength(1)
        expect(workspace[0]?.lines).toHaveLength(1)
        expect(
            workspace[0]?.lines[0]?.options.map((option) => option.supplierId),
        ).toEqual(["sup-a", "sup-b"])
    })

    it("多家供应商时默认不预填供应商", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
            }),
            basis({
                basisId: "b-b",
                supplierId: "sup-b",
                supplierName: "供应商B",
            }),
        ])
        expect(buildDefaultSourcingLines(workspace[0])[0]?.supplierId).toBe("")
    })

    it("唯一供应商时默认带出该供应商和最大可采购量", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
            }),
        ])
        expect(buildDefaultSourcingLines(workspace[0])[0]).toMatchObject({
            supplierId: "sup-a",
            quantity: "2",
            selected: true,
        })
    })
})

describe("buildPurchaseOrderPreviews", () => {
    it("同一供应商合并为一张预览采购单", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
                lines: [
                    {
                        salesOrderLineId: "line-1",
                        salesOrderRevisionLineId: "rline-1",
                        itemName: "礼盒",
                        salesQuantity: "2",
                        coveredQuantity: "0",
                        remainingQuantity: "2",
                        maxCreateQuantity: "2",
                        unit: "件",
                        unitCostGross: "10.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 1",
                    },
                    {
                        salesOrderLineId: "line-2",
                        salesOrderRevisionLineId: "rline-2",
                        itemName: "贺卡",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "5.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 2",
                    },
                ],
            }),
        ])
        const previews = buildPurchaseOrderPreviews(workspace[0], [
            {
                salesOrderLineId: "line-1",
                selected: true,
                quantity: "2",
                supplierId: "sup-a",
            },
            {
                salesOrderLineId: "line-2",
                selected: true,
                quantity: "1",
                supplierId: "sup-a",
            },
        ])
        expect(previews).toHaveLength(1)
        expect(previews[0]?.supplierName).toBe("供应商A")
        expect(previews[0]?.lines).toHaveLength(2)
        expect(previews[0]?.totals.gross).toBe("25.00")
    })

    it("不同供应商拆成多张预览采购单", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
                lines: [
                    {
                        salesOrderLineId: "line-1",
                        salesOrderRevisionLineId: "rline-1",
                        itemName: "礼盒",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "10.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 1",
                    },
                ],
            }),
            basis({
                basisId: "b-b",
                supplierId: "sup-b",
                supplierName: "供应商B",
                lines: [
                    {
                        salesOrderLineId: "line-2",
                        salesOrderRevisionLineId: "rline-2",
                        itemName: "贺卡",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "5.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 2",
                    },
                ],
            }),
        ])
        const previews = buildPurchaseOrderPreviews(workspace[0], [
            {
                salesOrderLineId: "line-1",
                selected: true,
                quantity: "1",
                supplierId: "sup-a",
            },
            {
                salesOrderLineId: "line-2",
                selected: true,
                quantity: "1",
                supplierId: "sup-b",
            },
        ])
        expect(previews).toHaveLength(2)
        expect(previews.map((preview) => preview.supplierId).sort()).toEqual([
            "sup-a",
            "sup-b",
        ])
    })
})

describe("commonSuppliersForSelected", () => {
    it("只返回勾选行共同具备的供应商", () => {
        const workspace = buildSourcingWorkspace([
            basis({
                basisId: "b-a",
                supplierId: "sup-a",
                supplierName: "供应商A",
                lines: [
                    {
                        salesOrderLineId: "line-1",
                        salesOrderRevisionLineId: "rline-1",
                        itemName: "礼盒",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "10.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 1",
                    },
                    {
                        salesOrderLineId: "line-2",
                        salesOrderRevisionLineId: "rline-2",
                        itemName: "贺卡",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "5.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 2",
                    },
                ],
            }),
            basis({
                basisId: "b-b",
                supplierId: "sup-b",
                supplierName: "供应商B",
                lines: [
                    {
                        salesOrderLineId: "line-1",
                        salesOrderRevisionLineId: "rline-1",
                        itemName: "礼盒",
                        salesQuantity: "1",
                        coveredQuantity: "0",
                        remainingQuantity: "1",
                        maxCreateQuantity: "1",
                        unit: "件",
                        unitCostGross: "9.00",
                        inputTaxRate: "0.13",
                        expectedDeliveryDate: "2026-09-01",
                        salesAllocationLabel: "销售明细 1",
                    },
                ],
            }),
        ])
        const common = commonSuppliersForSelected(workspace[0], [
            {
                salesOrderLineId: "line-1",
                selected: true,
                quantity: "1",
                supplierId: "",
            },
            {
                salesOrderLineId: "line-2",
                selected: true,
                quantity: "1",
                supplierId: "",
            },
        ])
        expect(common.map((option) => option.supplierId)).toEqual(["sup-a"])
    })
})

describe("sourcingQuantityError", () => {
    it("拒绝超过最大可创建量的数量", () => {
        expect(sourcingQuantityError("3", "2")).toContain("不能超过")
    })
})
