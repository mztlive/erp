import { describe, expect, it } from "vitest"

import { salesSelectionKeys } from "@/features/sales-selection/hooks/queries"

describe("sales selection query keys", () => {
    it("builds stable list keys from the filter query", () => {
        const first = salesSelectionKeys.list({ status: "PUBLISHED", page: 1 })
        const second = salesSelectionKeys.list({ status: "PUBLISHED", page: 1 })
        expect(first).toEqual(second)
        expect(first[0]).toBe("sales-selection")
    })

    it("separates detail, preview and proposal namespaces", () => {
        expect(salesSelectionKeys.detail("book_1")).toContain("book_1")
        expect(salesSelectionKeys.preview("token_1")).toContain("token_1")
        expect(salesSelectionKeys.proposal("proposal_1")).toContain(
            "proposal_1",
        )
        expect(salesSelectionKeys.detail("book_1")).not.toEqual(
            salesSelectionKeys.preview("book_1"),
        )
    })

    it("shares one root for targeted invalidation", () => {
        const root = salesSelectionKeys.all
        for (const key of [
            salesSelectionKeys.list({}),
            salesSelectionKeys.detail("book_1"),
            salesSelectionKeys.preview("token_1"),
            salesSelectionKeys.proposal("proposal_1"),
            salesSelectionKeys.session("book_1"),
        ]) {
            expect(key[0]).toBe(root[0])
        }
    })
})
