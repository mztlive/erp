import { describe, expect, it } from "vitest"

import {
    parsePage,
    parsePageSize,
} from "@/features/actual-profit-loss/lib/url-state"

describe("actual profit-loss pagination URL contract", () => {
    it("normalizes page to a positive one-based integer", () => {
        expect(parsePage("2.9")).toBe(2)
        expect(parsePage("0")).toBe(1)
        expect(parsePage("invalid")).toBe(1)
    })

    it("accepts only supported server page sizes", () => {
        expect(parsePageSize("50")).toBe(50)
        expect(parsePageSize("100")).toBe(100)
        expect(parsePageSize("25")).toBe(20)
    })
})
