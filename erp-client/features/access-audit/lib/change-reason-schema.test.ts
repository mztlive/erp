import { describe, expect, it } from "vitest"

import { changeReasonSchema } from "./change-reason-schema"

describe("changeReasonSchema", () => {
    it("accepts a reason code with an optional comment", () => {
        const result = changeReasonSchema.safeParse({
            reasonCode: "SECURITY_OPS",
            comment: "例行调整",
        })
        expect(result.success).toBe(true)
    })

    it("accepts an empty comment", () => {
        const result = changeReasonSchema.safeParse({
            reasonCode: "SECURITY_OPS",
            comment: "",
        })
        expect(result.success).toBe(true)
    })

    it("rejects a missing reason code", () => {
        const result = changeReasonSchema.safeParse({ reasonCode: "", comment: "" })
        expect(result.success).toBe(false)
    })

    it("rejects comments longer than 200 characters", () => {
        const result = changeReasonSchema.safeParse({
            reasonCode: "SECURITY_OPS",
            comment: "长".repeat(201),
        })
        expect(result.success).toBe(false)
    })
})
