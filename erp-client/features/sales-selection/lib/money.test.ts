import { describe, expect, it } from "vitest"

import {
    compareMoney,
    decrementQuantity,
    formatMoney,
    incrementQuantity,
    isValidQuantity,
    lineAmount,
    sumAmounts,
} from "@/features/sales-selection/lib/money"

describe("sales selection money", () => {
    it("computes line amount as unit price times copies", () => {
        expect(lineAmount("12.50", "3")).toBe("37.50")
        expect(lineAmount("100.00", "1")).toBe("100.00")
    })

    it("sums line amounts into a total", () => {
        expect(sumAmounts(["37.50", "100.00"])).toBe("137.50")
        expect(sumAmounts([])).toBe("0.00")
    })

    it("accepts quantities from 1 to 100000 as integers", () => {
        expect(isValidQuantity("1")).toBe(true)
        expect(isValidQuantity("100000")).toBe(true)
        expect(isValidQuantity("0")).toBe(false)
        expect(isValidQuantity("100001")).toBe(false)
        expect(isValidQuantity("2.5")).toBe(false)
        expect(isValidQuantity("")).toBe(false)
        expect(isValidQuantity("  ")).toBe(false)
    })

    it("steps quantities within bounds", () => {
        expect(incrementQuantity("1")).toBe("2")
        expect(incrementQuantity("100000")).toBe("100000")
        expect(decrementQuantity("2")).toBe("1")
        expect(decrementQuantity("1")).toBe("1")
    })

    it("rejects overflow instead of truncating", () => {
        expect(() =>
            lineAmount("792281625142643375935439503.35", "2"),
        ).toThrow()
        expect(() =>
            sumAmounts([
                "792281625142643375935439503.35",
                "792281625142643375935439503.35",
            ]),
        ).toThrow()
    })

    it("formats and compares without float leakage", () => {
        expect(formatMoney("137.50")).toContain("137.50")
        expect(compareMoney("100.00", "100.00")).toBe(0)
        expect(compareMoney("99.99", "100.00")).toBe(-1)
        expect(compareMoney("100.01", "100.00")).toBe(1)
    })
})
