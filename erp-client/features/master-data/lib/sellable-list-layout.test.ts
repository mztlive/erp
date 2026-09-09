import { describe, expect, it } from "vitest"

import { parseSellableListLayout } from "./sellable-list-layout"

describe("parseSellableListLayout", () => {
    it("only treats gallery as the alternate layout", () => {
        expect(parseSellableListLayout("gallery")).toBe("gallery")
        expect(parseSellableListLayout("table")).toBe("table")
        expect(parseSellableListLayout("cards")).toBe("table")
        expect(parseSellableListLayout(null)).toBe("table")
    })
})
