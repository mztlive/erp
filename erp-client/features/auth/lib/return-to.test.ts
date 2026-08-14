import { describe, expect, it } from "vitest"

import { DEFAULT_RETURN_TO, resolveReturnTarget } from "./return-to"

describe("resolveReturnTarget", () => {
    it("returns the default page when the param is absent or empty", () => {
        expect(resolveReturnTarget(null)).toBe(DEFAULT_RETURN_TO)
        expect(resolveReturnTarget("")).toBe(DEFAULT_RETURN_TO)
    })

    it("accepts in-app absolute paths, including query strings", () => {
        expect(resolveReturnTarget("/workspace/tasks")).toBe("/workspace/tasks")
        expect(resolveReturnTarget("/workspace/mall?tab=orders")).toBe(
            "/workspace/mall?tab=orders",
        )
    })

    it("rejects protocol-relative URLs", () => {
        expect(resolveReturnTarget("//evil.example/steal")).toBe(
            DEFAULT_RETURN_TO,
        )
    })

    it("rejects relative and absolute external URLs", () => {
        expect(resolveReturnTarget("workspace/tasks")).toBe(DEFAULT_RETURN_TO)
        expect(resolveReturnTarget("https://evil.example")).toBe(
            DEFAULT_RETURN_TO,
        )
    })
})
