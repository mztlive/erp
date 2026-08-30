import { describe, expect, it } from "vitest"

import {
    divideFixed,
    formatCurrencyFixed,
    formatFixedDisplay,
    normalizeFixed,
    splitGrossByPercentRate,
    subtractFixed,
} from "@/lib/fixed-decimal"

describe("fixed decimal arithmetic", () => {
    it("uses signed half-even rounding at the requested scale", () => {
        expect(normalizeFixed("1.005", { maxScale: 3, outputScale: 2 })).toBe(
            "1.00",
        )
        expect(normalizeFixed("1.015", { maxScale: 3, outputScale: 2 })).toBe(
            "1.02",
        )
        expect(
            normalizeFixed("-1.005", {
                maxScale: 3,
                outputScale: 2,
                allowNegative: true,
            }),
        ).toBe("-1.00")
    })

    it("divides and subtracts without passing through JavaScript number", () => {
        expect(
            divideFixed("1", "8", {
                numeratorMaxScale: 0,
                denominatorMaxScale: 0,
                outputScale: 2,
            }),
        ).toBe("0.12")
        expect(
            divideFixed("3", "8", {
                numeratorMaxScale: 0,
                denominatorMaxScale: 0,
                outputScale: 2,
            }),
        ).toBe("0.38")
        expect(
            subtractFixed("10000000000000000.01", "0.02", {
                maxScale: 2,
                outputScale: 2,
            }),
        ).toBe("9999999999999999.99")
    })

    it("splits tax from gross with the backend banker-rounding contract", () => {
        expect(splitGrossByPercentRate("113.00", "13")).toEqual({
            gross: "113.00",
            net: "100.00",
            tax: "13.00",
        })
    })

    it("formats display values without losing precision through number", () => {
        expect(
            formatCurrencyFixed("10000000000000000.01", {
                maxScale: 2,
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
            }),
        ).toBe("¥10,000,000,000,000,000.01")
        expect(
            formatFixedDisplay("-1234.5000", {
                maxScale: 4,
                minimumFractionDigits: 2,
                maximumFractionDigits: 4,
            }),
        ).toBe("-1,234.50")
    })
})
