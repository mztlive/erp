import { describe, it, expect } from "vitest"

import {
    readOnlyNote,
    responsibilityStatus,
    responsibilityStatusLabel,
    sourceReturnHref,
} from "./presentation"
import { makeOperation } from "../hooks/test-data"

describe("responsibilityStatus", () => {
    it("is blocked when the gate is BLOCKED", () => {
        const operation = makeOperation({ gate: { state: "BLOCKED" } })
        expect(responsibilityStatus(operation, true)).toBe("blocked")
    })

    it("is assigned_to_me for executors and assigned_to_other otherwise", () => {
        const operation = makeOperation()
        expect(responsibilityStatus(operation, true)).toBe("assigned_to_me")
        expect(responsibilityStatus(operation, false)).toBe(
            "assigned_to_other",
        )
        expect(responsibilityStatus(undefined, false)).toBe(
            "assigned_to_other",
        )
    })
})

describe("responsibilityStatusLabel", () => {
    it("speaks read-only, blocked and processable in business terms", () => {
        expect(responsibilityStatusLabel(makeOperation(), false)).toBe(
            "只能查看",
        )
        expect(
            responsibilityStatusLabel(
                makeOperation({ gate: { state: "BLOCKED" } }),
                true,
            ),
        ).toBe("业务条件未满足")
        expect(responsibilityStatusLabel(makeOperation(), true)).toBe(
            "当前岗位可处理",
        )
    })
})

describe("readOnlyNote", () => {
    it("names the responsible party and the due date", () => {
        expect(readOnlyNote(makeOperation())).toBe(
            "你只能查看。这条由 仓储 · 周航 处理，预计 今天 10:00 前完成。",
        )
    })

    it("marks overdue items explicitly", () => {
        expect(
            readOnlyNote(makeOperation({ overdue: true })),
        ).toBe(
            "你只能查看。这条由 仓储 · 周航 处理，原定 今天 10:00，已超期。",
        )
    })

    it("falls back to a generic note without an operation", () => {
        expect(readOnlyNote(undefined)).toBe("你只能查看这些单据的进度。")
    })
})

describe("sourceReturnHref", () => {
    it("prefers the explicit returnTo", () => {
        expect(
            sourceReturnHref("/custom/back", "W05", makeOperation()),
        ).toBe("/custom/back")
    })

    it("returns to the sales order for W05 deep links", () => {
        expect(sourceReturnHref(undefined, "W05", makeOperation())).toBe(
            "/sales/orders/so_1",
        )
    })

    it("returns to procurement orders for W08 deep links", () => {
        expect(sourceReturnHref(undefined, "W08", makeOperation())).toBe(
            "/procurement/orders",
        )
    })

    it("returns to inventory for W10 deep links", () => {
        expect(sourceReturnHref(undefined, "W10", makeOperation())).toBe(
            "/inventory",
        )
    })

    it("stays on the page without a return target", () => {
        expect(
            sourceReturnHref(undefined, "W01", makeOperation()),
        ).toBeUndefined()
    })
})
