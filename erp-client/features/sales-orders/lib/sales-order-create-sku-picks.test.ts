import { describe, expect, it } from "vitest"

import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import { createEmptyLine } from "@/features/sales-orders/lib/sales-order-create-model"
import {
    appendSellablePicksToLines,
    applySellablePicksToLines,
    replaceLineWithSellablePick,
} from "@/features/sales-orders/lib/sales-order-create-sku-picks"

function pick(
    overrides: Partial<SellableSkuPick> & Pick<SellableSkuPick, "skuId">,
): SellableSkuPick {
    return {
        skuRevisionId: `${overrides.skuId}-rev`,
        skuNo: `NO-${overrides.skuId}`,
        name: overrides.name ?? `商品 ${overrides.skuId}`,
        specificationLabel: "颜色：红",
        baseUnit: "件",
        salesVisiblePriceGross: "12.00",
        ...overrides,
    }
}

describe("appendSellablePicksToLines", () => {
    it("fills the empty placeholder line before appending", () => {
        const empty = createEmptyLine("physical_service")
        const next = appendSellablePicksToLines(
            [empty],
            [
                pick({ skuId: "sku-1" }),
                pick({ skuId: "sku-2", name: "第二件" }),
            ],
            "physical_service",
        )
        expect(next).toHaveLength(2)
        expect(next[0]?.sku).toBe("sku-1")
        expect(next[0]?.rowKey).toBe(empty.rowKey)
        expect(next[0]?.unit).toBe("件")
        expect(next[0]?.unitPriceGross).toBe("12.00")
        expect(next[1]?.sku).toBe("sku-2")
        expect(next[1]?.name).toBe("第二件")
        expect(next[1]?.rowKey).not.toBe(empty.rowKey)
    })

    it("keeps already filled lines and appends the rest", () => {
        const filled = {
            ...createEmptyLine("physical_service"),
            name: "已有",
            sku: "sku-old",
            skuRevisionId: "rev-old",
            unit: "盒",
            quantity: "3",
        }
        const next = appendSellablePicksToLines(
            [filled],
            [pick({ skuId: "sku-new" })],
            "physical_service",
        )
        expect(next).toHaveLength(2)
        expect(next[0]?.sku).toBe("sku-old")
        expect(next[0]?.quantity).toBe("3")
        expect(next[1]?.sku).toBe("sku-new")
    })
})

describe("replaceLineWithSellablePick", () => {
    it("replaces identity fields but keeps quantity", () => {
        const line = {
            ...createEmptyLine("physical_service"),
            name: "旧名",
            sku: "sku-old",
            skuRevisionId: "rev-old",
            quantity: "5",
            unit: "盒",
            unitPriceGross: "9.00",
        }
        const [replaced] = replaceLineWithSellablePick(
            [line],
            0,
            pick({ skuId: "sku-new", name: "新名" }),
            "physical_service",
        )
        expect(replaced?.sku).toBe("sku-new")
        expect(replaced?.name).toBe("新名")
        expect(replaced?.quantity).toBe("5")
        expect(replaced?.rowKey).toBe(line.rowKey)
        expect(replaced?.unit).toBe("件")
        expect(replaced?.unitPriceGross).toBe("12.00")
    })
})

describe("applySellablePicksToLines", () => {
    it("replaces the focused row and appends the rest", () => {
        const filled = {
            ...createEmptyLine("physical_service"),
            name: "已有",
            sku: "sku-old",
            skuRevisionId: "rev-old",
            quantity: "2",
        }
        const next = applySellablePicksToLines(
            [filled],
            [
                pick({ skuId: "sku-a", name: "甲" }),
                pick({ skuId: "sku-b", name: "乙" }),
            ],
            "physical_service",
            0,
        )
        expect(next).toHaveLength(2)
        expect(next[0]?.sku).toBe("sku-a")
        expect(next[0]?.name).toBe("甲")
        expect(next[0]?.quantity).toBe("2")
        expect(next[0]?.rowKey).toBe(filled.rowKey)
        expect(next[1]?.sku).toBe("sku-b")
        expect(next[1]?.name).toBe("乙")
    })
})
