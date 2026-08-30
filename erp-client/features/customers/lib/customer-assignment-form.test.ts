import { describe, expect, it } from "vitest"

import type { CustomerAssignmentView } from "@/features/customers/types"
import { customerAssignmentSchema } from "./customer-assignment-form"

const target: CustomerAssignmentView = {
    id: "assignment-1",
    role: "COLLABORATOR",
    userId: "user-1",
    userName: "张三",
    effectiveFrom: "2026-08-01",
    changeReason: "协作",
    version: 3,
    isCurrent: true,
}

describe("customerAssignmentSchema", () => {
    it("requires an owner, effective date and reason when assigning", () => {
        const result = customerAssignmentSchema().safeParse({
            userId: "",
            role: "COLLABORATOR",
            effectiveFrom: "",
            effectiveTo: "",
            reason: "  ",
        })

        expect(result.success).toBe(false)
        if (result.success) return
        expect(result.error.issues.map((issue) => issue.path[0])).toEqual(
            expect.arrayContaining(["userId", "effectiveTo", "reason"]),
        )
    })

    it("requires an end date strictly after the existing assignment start", () => {
        const invalid = customerAssignmentSchema(target).safeParse({
            userId: "",
            role: "COLLABORATOR",
            effectiveFrom: "2026-08-30",
            effectiveTo: "2026-08-01",
            reason: "结束协作",
        })
        const valid = customerAssignmentSchema(target).safeParse({
            userId: "",
            role: "COLLABORATOR",
            effectiveFrom: "2026-08-30",
            effectiveTo: "2026-08-02",
            reason: "结束协作",
        })

        expect(invalid.success).toBe(false)
        expect(valid.success).toBe(true)
    })
})
