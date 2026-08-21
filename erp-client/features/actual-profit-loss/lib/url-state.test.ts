import { describe, it, expect } from "vitest"

import {
    basisLabel,
    coveragePercentNumber,
    mapFreshnessState,
    parseCoverage,
    parseCsvValues,
    parseDimension,
    parsePreset,
    resolvePeriod,
    serializeCsvValues,
} from "@/features/actual-profit-loss/lib/url-state"

function pad(n: number): string {
    return String(n).padStart(2, "0")
}

function toISODate(date: Date): string {
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

describe("parseCoverage", () => {
    it("accepts the three known coverage values", () => {
        expect(parseCoverage("covered")).toBe("covered")
        expect(parseCoverage("uncovered")).toBe("uncovered")
        expect(parseCoverage("all")).toBe("all")
    })

    it("falls back to covered for missing or unknown values", () => {
        expect(parseCoverage(null)).toBe("covered")
        expect(parseCoverage("banana")).toBe("covered")
        expect(parseCoverage("")).toBe("covered")
    })
})

describe("parseDimension", () => {
    it("accepts every known dimension", () => {
        expect(parseDimension("sales_order")).toBe("sales_order")
        expect(parseDimension("customer")).toBe("customer")
        expect(parseDimension("scenario")).toBe("scenario")
        expect(parseDimension("fulfillment")).toBe("fulfillment")
        expect(parseDimension("cost_type")).toBe("cost_type")
    })

    it("falls back to sales_order for missing or unknown values", () => {
        expect(parseDimension(null)).toBe("sales_order")
        expect(parseDimension("banana")).toBe("sales_order")
    })
})

describe("parsePreset", () => {
    it("accepts the three known presets", () => {
        expect(parsePreset("month-to-date")).toBe("month-to-date")
        expect(parsePreset("last-month")).toBe("last-month")
        expect(parsePreset("quarter-to-date")).toBe("quarter-to-date")
    })

    it("falls back to month-to-date for missing or unknown values", () => {
        expect(parsePreset(null)).toBe("month-to-date")
        expect(parsePreset("banana")).toBe("month-to-date")
    })
})

describe("resolvePeriod", () => {
    const now = new Date()

    it("resolves month-to-date to the current month range", () => {
        const range = resolvePeriod("month-to-date")
        expect(range.from).toBe(
            toISODate(new Date(now.getFullYear(), now.getMonth(), 1)),
        )
        expect(range.to).toBe(toISODate(now))
    })

    it("resolves last-month to the previous month's full range", () => {
        const range = resolvePeriod("last-month")
        const lastMonthFirst = new Date(now.getFullYear(), now.getMonth() - 1, 1)
        const firstOfThisMonth = new Date(now.getFullYear(), now.getMonth(), 1)
        const lastOfLastMonth = new Date(firstOfThisMonth.getTime() - 1)
        expect(range.from).toBe(toISODate(lastMonthFirst))
        expect(range.to).toBe(toISODate(lastOfLastMonth))
    })

    it("resolves quarter-to-date from the quarter start to today", () => {
        const range = resolvePeriod("quarter-to-date")
        const quarterStartMonth = Math.floor(now.getMonth() / 3) * 3
        expect(range.from).toBe(
            toISODate(new Date(now.getFullYear(), quarterStartMonth, 1)),
        )
        expect(range.to).toBe(toISODate(now))
    })
})

describe("basisLabel", () => {
    it("maps known basis codes to Chinese labels", () => {
        expect(basisLabel("sales_revenue_recognition_date")).toBe(
            "销售收入确认日",
        )
        expect(basisLabel("sales_order_effective_date")).toBe("销售单生效日")
        expect(basisLabel("fulfillment_complete_date")).toBe("履约完成日")
        expect(basisLabel("cost_occurred_date")).toBe("成本发生日")
    })

    it("passes through unknown codes unchanged", () => {
        expect(basisLabel("banana")).toBe("banana")
    })
})

describe("mapFreshnessState", () => {
    it("prioritizes the refreshing flag", () => {
        expect(
            mapFreshnessState("stale", { refreshing: true }),
        ).toEqual({ uiState: "syncing", statusLabel: "正在刷新数据" })
    })

    it("prioritizes the refreshFailed flag after refreshing", () => {
        expect(
            mapFreshnessState("fresh", { refreshFailed: true }),
        ).toEqual({ uiState: "failed", statusLabel: "刷新失败 · 保留旧数据" })
    })

    it("maps each projection state to its UI state", () => {
        expect(mapFreshnessState("stale")).toEqual({
            uiState: "stale",
            statusLabel: "数据陈旧 · 来源更新时间已超前",
        })
        expect(mapFreshnessState("rebuilding")).toEqual({
            uiState: "syncing",
            statusLabel: "数据更新中",
        })
        expect(mapFreshnessState("failed")).toEqual({
            uiState: "failed",
            statusLabel: "数据更新失败",
        })
        expect(mapFreshnessState("fresh")).toEqual({
            uiState: "fresh",
            statusLabel: "数据已更新",
        })
    })
})

describe("parseCsvValues", () => {
    it("splits, trims and dedupes comma-separated values", () => {
        expect(parseCsvValues("电子交付, 公司仓发 ,电子交付")).toEqual([
            "公司仓发",
            "电子交付",
        ])
    })

    it("sorts values for stable serialization", () => {
        expect(parseCsvValues("printing,logistics")).toEqual([
            "logistics",
            "printing",
        ])
    })

    it("returns an empty list for missing or blank input", () => {
        expect(parseCsvValues(null)).toEqual([])
        expect(parseCsvValues("")).toEqual([])
        expect(parseCsvValues(" , ,")).toEqual([])
    })
})

describe("serializeCsvValues", () => {
    it("joins deduped sorted values", () => {
        expect(serializeCsvValues(["电子交付", "公司仓发", "电子交付"])).toBe(
            "公司仓发,电子交付",
        )
    })

    it("returns an empty string for no values", () => {
        expect(serializeCsvValues([])).toBe("")
        expect(serializeCsvValues(["  ", ""])).toBe("")
    })
})

describe("coveragePercentNumber", () => {
    it("parses percentage strings into clamped numbers", () => {
        expect(coveragePercentNumber("100%")).toBe(100)
        expect(coveragePercentNumber("57.3%")).toBe(57.3)
        expect(coveragePercentNumber("0%")).toBe(0)
    })

    it("clamps to the 0-100 range", () => {
        expect(coveragePercentNumber("200%")).toBe(100)
        expect(coveragePercentNumber("-5%")).toBe(0)
    })

    it("returns 0 for unparseable values", () => {
        expect(coveragePercentNumber("abc")).toBe(0)
        expect(coveragePercentNumber("")).toBe(0)
    })
})
