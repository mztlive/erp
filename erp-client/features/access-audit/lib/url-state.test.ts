import { describe, expect, it } from "vitest"

import { parseView } from "./url-state"

describe("parseView", () => {
    it("accepts each known view value", () => {
        expect(parseView("roles")).toBe("roles")
        expect(parseView("users")).toBe("users")
        expect(parseView("scopes")).toBe("scopes")
        expect(parseView("audit")).toBe("audit")
    })

    it("falls back to roles for unknown or missing values", () => {
        expect(parseView("fields")).toBe("roles")
        expect(parseView("bogus")).toBe("roles")
        expect(parseView(null)).toBe("roles")
        expect(parseView("")).toBe("roles")
    })
})
