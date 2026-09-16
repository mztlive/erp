import { describe, expect, it } from "vitest"

import {
    isCurrentDimension,
    isHistoryDimension,
    parseCaliber,
    parseCsvIds,
    parseCurrentDimension,
    parseCurrentSort,
    parseDualBoolean,
    parseDualPage,
    parseDualPageSize,
    parseHistoryDimension,
    parseHistorySort,
    serializeCsvIds,
} from "@/features/customer-quality/lib/dual-url-state"
import { toDualEmptyReason } from "@/features/customer-quality/dual-types"

describe("customer-quality dual caliber URL contract", () => {
    it("defaults to current caliber and never mixes history params", () => {
        expect(parseCaliber(null)).toBe("current")
        expect(parseCaliber("history")).toBe("history")
        expect(parseCaliber("owner_user")).toBe("current")
    })

    it("parses person/org id lists deduplicated and sorted", () => {
        expect(parseCsvIds("u-2,u-1,u-2")).toEqual(["u-1", "u-2"])
        expect(parseCsvIds("  ")).toEqual([])
        expect(parseCsvIds(null)).toEqual([])
        expect(serializeCsvIds(["u-2", "u-1", "u-2"])).toBe("u-2,u-1")
    })

    it("keeps current and history dimensions out of each other's caliber", () => {
        expect(parseCurrentDimension("attribution_user")).toBe("customer")
        expect(parseHistoryDimension("owner_user")).toBe("attribution_user")
        expect(parseCurrentDimension("owner_org")).toBe("owner_org")
        expect(parseHistoryDimension("attribution_org")).toBe("attribution_org")
        expect(isCurrentDimension("attribution_user")).toBe(false)
        expect(isHistoryDimension("owner_user")).toBe(false)
    })

    it("rejects cross-caliber sort fields back to the default", () => {
        expect(parseCurrentSort("label:asc")).toBe("label:asc")
        expect(parseHistorySort("customerNo:asc")).toBe("orderCount:desc")
        expect(parseHistorySort("orderCount:sideways")).toBe("orderCount:desc")
        expect(parseCurrentSort(null)).toBe("orderCount:desc")
    })

    it("normalizes dual pagination and descendant flags", () => {
        expect(parseDualPage("3")).toBe(3)
        expect(parseDualPage("0")).toBe(1)
        expect(parseDualPage("nope")).toBe(1)
        expect(parseDualPageSize("50")).toBe(50)
        expect(parseDualPageSize("25")).toBe(20)
        expect(parseDualBoolean("true")).toBe(true)
        expect(parseDualBoolean("yes")).toBeUndefined()
    })

    it("maps backend empty reasons to the three UI states", () => {
        expect(toDualEmptyReason("no_scope")).toBe("no-scope")
        expect(toDualEmptyReason("filtered_empty")).toBe("filtered-empty")
        expect(toDualEmptyReason("no_data")).toBe("no-data")
        expect(toDualEmptyReason("unknown")).toBeNull()
    })
})
