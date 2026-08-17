import { describe, expect, it } from "vitest"

import { canOpenProcurementConfirmPlan } from "./actions"

describe("canOpenProcurementConfirmPlan", () => {
    it("allows opening the plan dialog once SAVE is available", () => {
        expect(canOpenProcurementConfirmPlan(["SAVE", "REJECT"])).toBe(true)
    })

    it("allows opening the plan dialog when APPROVE is already granted", () => {
        expect(canOpenProcurementConfirmPlan(["APPROVE"])).toBe(true)
    })

    it("does not open the plan dialog before the operator can work the confirmation", () => {
        expect(canOpenProcurementConfirmPlan(["START_PROCESSING"])).toBe(false)
        expect(canOpenProcurementConfirmPlan(["REJECT"])).toBe(false)
        expect(canOpenProcurementConfirmPlan([])).toBe(false)
        expect(canOpenProcurementConfirmPlan(undefined)).toBe(false)
    })
})
