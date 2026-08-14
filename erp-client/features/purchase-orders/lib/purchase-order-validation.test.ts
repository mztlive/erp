import { describe, it, expect } from "vitest"

import { positiveDecimal, taxRateValid } from "./purchase-order-validation"

describe("positiveDecimal", () => {
    it("接受合法正数", () => {
        expect(positiveDecimal("1")).toBe(true)
        expect(positiveDecimal("1.5")).toBe(true)
        expect(positiveDecimal("0.01")).toBe(true)
    })

    it("拒绝零、负数与非数字", () => {
        expect(positiveDecimal("0")).toBe(false)
        expect(positiveDecimal("-1")).toBe(false)
        expect(positiveDecimal("abc")).toBe(false)
        expect(positiveDecimal("1e3")).toBe(false)
    })
})

describe("taxRateValid", () => {
    it("接受空值与 0-1 之间的小数", () => {
        expect(taxRateValid("")).toBe(true)
        expect(taxRateValid("0")).toBe(false)
        expect(taxRateValid("0.13")).toBe(true)
        expect(taxRateValid("0.5")).toBe(true)
    })

    it("拒绝 1 及以上与非法值", () => {
        expect(taxRateValid("1")).toBe(false)
        expect(taxRateValid("1.2")).toBe(false)
        expect(taxRateValid("x")).toBe(false)
    })
})
