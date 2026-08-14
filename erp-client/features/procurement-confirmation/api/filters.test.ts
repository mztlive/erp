import { describe, expect, it } from "vitest"

import { isServerIssuedQueueContextId } from "./filters"

describe("isServerIssuedQueueContextId", () => {
    it("accepts a 64-character hex digest", () => {
        expect(
            isServerIssuedQueueContextId(
                "89cbbe65b947589c15920e01adef5faf4cb6d33d951e120530b6e26e03311fdb",
            ),
        ).toBe(true)
    })

    it("rejects client placeholders and empty values", () => {
        expect(
            isServerIssuedQueueContextId("queue:procurement-confirmation:mine"),
        ).toBe(false)
        expect(isServerIssuedQueueContextId("")).toBe(false)
        expect(isServerIssuedQueueContextId(undefined)).toBe(false)
        expect(isServerIssuedQueueContextId(null)).toBe(false)
    })
})
