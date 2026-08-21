import { describe, expect, it } from "vitest"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    buildSettlementFilterChips,
    hasAppliedSettlementFilters,
    hasStructuredSettlementFilters,
    joinSettlementStatusParam,
    parseSettlementStatusParam,
    validateSettlementPeriodRange,
} from "./settlement-list-filters"

function makeState(
    overrides: Partial<SettlementsUrlState> = {},
): SettlementsUrlState {
    return {
        view: "pending",
        page: 1,
        section: "overview",
        ...overrides,
    }
}

const SUPPLIERS = [
    { supplierId: "sup1", supplierName: "上海材料供应" },
    { supplierId: "sup2", supplierName: "深圳电子元件" },
]

describe("parseSettlementStatusParam", () => {
    it("splits comma separated values", () => {
        expect(parseSettlementStatusParam("DRAFT,PENDING_REVIEW")).toEqual([
            "DRAFT",
            "PENDING_REVIEW",
        ])
    })

    it("drops illegal enum values and duplicates", () => {
        expect(
            parseSettlementStatusParam("UNKNOWN,DRAFT,DRAFT,  VOIDED "),
        ).toEqual(["DRAFT", "VOIDED"])
    })

    it("returns an empty array for missing or empty input", () => {
        expect(parseSettlementStatusParam(undefined)).toEqual([])
        expect(parseSettlementStatusParam("")).toEqual([])
        expect(parseSettlementStatusParam("UNKNOWN")).toEqual([])
    })
})

describe("joinSettlementStatusParam", () => {
    it("joins valid values into a comma separated url value", () => {
        expect(
            joinSettlementStatusParam(["DRAFT", "PENDING_REVIEW"]),
        ).toBe("DRAFT,PENDING_REVIEW")
    })

    it("omits invalid values and returns undefined when nothing remains", () => {
        expect(joinSettlementStatusParam(["UNKNOWN", ""])).toBeUndefined()
    })
})

describe("hasStructuredSettlementFilters / hasAppliedSettlementFilters", () => {
    it("treats view as saved view, not a filter", () => {
        expect(hasAppliedSettlementFilters(makeState({ view: "confirmed" }))).toBe(
            false,
        )
        expect(
            hasStructuredSettlementFilters(makeState({ view: "confirmed" })),
        ).toBe(false)
    })

    it("detects every structured condition", () => {
        for (const patch of [
            { supplierId: "sup1" },
            { status: "DRAFT" },
            { differenceType: "AMOUNT" as const },
            { periodFrom: "2026-01-01" },
            { periodTo: "2026-01-31" },
        ]) {
            expect(
                hasStructuredSettlementFilters(makeState(patch)),
            ).toBe(true)
            expect(hasAppliedSettlementFilters(makeState(patch))).toBe(true)
        }
    })

    it("counts the keyword as an applied filter but not a structured one", () => {
        const state = makeState({ q: "abc" })
        expect(hasStructuredSettlementFilters(state)).toBe(false)
        expect(hasAppliedSettlementFilters(state)).toBe(true)
    })
})

describe("validateSettlementPeriodRange", () => {
    it("accepts an empty or open range", () => {
        expect(validateSettlementPeriodRange("", "")).toBeNull()
        expect(validateSettlementPeriodRange("2026-01-01", "")).toBeNull()
        expect(validateSettlementPeriodRange("", "2026-01-31")).toBeNull()
    })

    it("accepts a closed range in order", () => {
        expect(
            validateSettlementPeriodRange("2026-01-01", "2026-01-31"),
        ).toBeNull()
    })

    it("rejects a range where the start is after the end", () => {
        expect(
            validateSettlementPeriodRange("2026-02-01", "2026-01-31"),
        ).toBe("期间开始日期不能晚于结束日期")
    })
})

describe("buildSettlementFilterChips", () => {
    it("renders no chips for an empty state", () => {
        expect(buildSettlementFilterChips(makeState(), SUPPLIERS)).toEqual([])
    })

    it("renders every applied condition as a removable chip", () => {
        const chips = buildSettlementFilterChips(
            makeState({
                q: " st-1 ",
                supplierId: "sup1",
                status: "DRAFT,PENDING_REVIEW",
                differenceType: "AMOUNT",
                periodFrom: "2026-01-01",
                periodTo: "2026-01-31",
            }),
            SUPPLIERS,
        )

        expect(chips).toEqual([
            { key: "q", label: "搜索：st-1" },
            { key: "supplierId", label: "供应商：上海材料供应" },
            { key: "status", label: "状态：草稿、待复核" },
            { key: "differenceType", label: "差异类型：金额差异" },
            { key: "period", label: "期间：2026-01-01 至 2026-01-31" },
        ])
    })

    it("falls back to the raw id when the supplier name is unknown", () => {
        const chips = buildSettlementFilterChips(
            makeState({ supplierId: "sup9" }),
            SUPPLIERS,
        )
        expect(chips).toEqual([{ key: "supplierId", label: "供应商：sup9" }])
    })

    it("labels an open-ended period range with 不限", () => {
        const chips = buildSettlementFilterChips(
            makeState({ periodFrom: "2026-01-01" }),
            SUPPLIERS,
        )
        expect(chips).toEqual([
            { key: "period", label: "期间：2026-01-01 至 不限" },
        ])
    })
})
