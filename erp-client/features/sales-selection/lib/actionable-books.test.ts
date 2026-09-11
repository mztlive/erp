import { describe, expect, it } from "vitest"

import {
    ACTIONABLE_BOOK_STATUSES,
    isActionableBookStatus,
    sumActionableBookCounts,
} from "@/features/sales-selection/lib/actionable-books"

describe("actionable book statuses", () => {
    it("counts draft, preparing and pending publish", () => {
        expect([...ACTIONABLE_BOOK_STATUSES]).toEqual([
            "DRAFT",
            "PREPARING",
            "PENDING_PUBLISH",
        ])
        expect(isActionableBookStatus("DRAFT")).toBe(true)
        expect(isActionableBookStatus("PREPARING")).toBe(true)
        expect(isActionableBookStatus("PENDING_PUBLISH")).toBe(true)
    })

    it("excludes published and terminal books", () => {
        expect(isActionableBookStatus("PUBLISHED")).toBe(false)
        expect(isActionableBookStatus("SUBMITTED")).toBe(false)
        expect(isActionableBookStatus("CLOSED")).toBe(false)
        expect(isActionableBookStatus("VOIDED")).toBe(false)
    })

    it("sums per-status totals for the nav badge", () => {
        expect(sumActionableBookCounts([2, 1, 4])).toBe(7)
        expect(sumActionableBookCounts([0, 0, 0])).toBe(0)
    })
})
