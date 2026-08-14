import { describe, expect, it } from "vitest"

import {
    parseBusinessType,
    parseFundsReview,
    parseScenario,
} from "./url-state"

describe("parseScenario", () => {
    it("accepts every known scenario value", () => {
        expect(parseScenario("default")).toBe("default")
        expect(parseScenario("no_period_default")).toBe("no_period_default")
        expect(parseScenario("empty")).toBe("empty")
        expect(parseScenario("no_scope")).toBe("no_scope")
        expect(parseScenario("forbidden")).toBe("forbidden")
        expect(parseScenario("field_denied")).toBe("field_denied")
        expect(parseScenario("stale")).toBe("stale")
        expect(parseScenario("rebuilding")).toBe("rebuilding")
        expect(parseScenario("failed")).toBe("failed")
        expect(parseScenario("refresh_failed")).toBe("refresh_failed")
    })

    it("rejects missing, empty and unknown values", () => {
        expect(parseScenario(null)).toBeUndefined()
        expect(parseScenario("")).toBeUndefined()
        expect(parseScenario("bogus")).toBeUndefined()
        expect(parseScenario("default ")).toBeUndefined()
    })
})

describe("parseFundsReview", () => {
    it("maps reviewed_only and falls back to all", () => {
        expect(parseFundsReview("reviewed_only")).toBe("reviewed_only")
        expect(parseFundsReview("all")).toBe("all")
        expect(parseFundsReview(null)).toBe("all")
        expect(parseFundsReview("")).toBe("all")
        expect(parseFundsReview("whatever")).toBe("all")
    })
})

describe("parseBusinessType", () => {
    it("accepts only VOUCHER and GOODS_SERVICE", () => {
        expect(parseBusinessType("VOUCHER")).toBe("VOUCHER")
        expect(parseBusinessType("GOODS_SERVICE")).toBe("GOODS_SERVICE")
    })

    it("rejects missing and unknown values", () => {
        expect(parseBusinessType(null)).toBeUndefined()
        expect(parseBusinessType("")).toBeUndefined()
        expect(parseBusinessType("voucher")).toBeUndefined()
        expect(parseBusinessType("OTHER")).toBeUndefined()
    })
})
