import { describe, expect, it } from "vitest"

import {
    bookIdentity,
    bookletStatusTone,
    formatBookInstant,
    publicSelectionHref,
} from "@/features/sales-selection/lib/presentation"

describe("bookletStatusTone", () => {
    it("maps lifecycle states to distinct tones", () => {
        expect(bookletStatusTone("DRAFT")).toBe("neutral")
        expect(bookletStatusTone("PREPARING")).toBe("warning")
        expect(bookletStatusTone("PENDING_PUBLISH")).toBe("warning")
        expect(bookletStatusTone("PUBLISHED")).toBe("info")
        expect(bookletStatusTone("SUBMITTED")).toBe("success")
        expect(bookletStatusTone("CLOSED")).toBe("void")
        expect(bookletStatusTone("VOIDED")).toBe("void")
    })
})

describe("formatBookInstant", () => {
    it("keeps qualification dates as calendar days", () => {
        expect(formatBookInstant("2026-09-11")).toBe("2026-09-11")
    })

    it("renders unix seconds as local date-time", () => {
        expect(formatBookInstant(1_704_067_200)).not.toBe("—")
        expect(formatBookInstant(1_704_067_200)).not.toBe("1704067200")
    })

    it("returns an em dash for empty values", () => {
        expect(formatBookInstant(null)).toBe("—")
        expect(formatBookInstant(undefined)).toBe("—")
        expect(formatBookInstant("")).toBe("—")
    })
})

describe("publicSelectionHref", () => {
    it("passes through absolute urls", () => {
        expect(publicSelectionHref("https://erp.example/s/abc")).toBe(
            "https://erp.example/s/abc",
        )
    })

    it("prefixes origin onto public paths", () => {
        expect(publicSelectionHref("/s/abc", "https://erp.example")).toBe(
            "https://erp.example/s/abc",
        )
    })

    it("returns null when the link is missing", () => {
        expect(publicSelectionHref(null)).toBeNull()
        expect(publicSelectionHref("  ")).toBeNull()
    })
})

describe("bookIdentity", () => {
    it("prefers book_id and falls back to id", () => {
        expect(bookIdentity({ id: "a", book_id: "b" })).toBe("b")
        expect(bookIdentity({ id: "a" })).toBe("a")
    })
})
