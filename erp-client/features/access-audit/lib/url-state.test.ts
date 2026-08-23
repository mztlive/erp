import { describe, expect, it } from "vitest"

import { parseView } from "./url-state"

describe("parseView", () => {
    it("accepts each known view value", () => {
        expect(parseView("roles")).toBe("roles")
        expect(parseView("users")).toBe("users")
        // 数据范围已收进主体详情，旧链接回退到角色视图
        expect(parseView("scopes")).toBe("roles")
        expect(parseView("audit")).toBe("audit")
    })

    it("falls back to roles for unknown or missing values", () => {
        expect(parseView("fields")).toBe("roles")
        expect(parseView("bogus")).toBe("roles")
        expect(parseView(null)).toBe("roles")
        expect(parseView("")).toBe("roles")
    })
})
