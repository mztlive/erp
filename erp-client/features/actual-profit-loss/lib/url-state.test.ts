import { describe, expect, it, vi } from "vitest"

import {
    resolvePeriod,
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

it("uses the Shanghai business day near UTC midnight", () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-08-31T16:30:00Z"))
    expect(resolvePeriod("month-to-date")).toEqual({
        from: "2026-09-01",
        to: "2026-09-01",
    })
    expect(resolvePeriod("last-month")).toEqual({
        from: "2026-08-01",
        to: "2026-08-31",
    })
    vi.useRealTimers()
})
