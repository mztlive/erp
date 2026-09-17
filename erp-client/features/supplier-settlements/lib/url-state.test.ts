import { describe, expect, it } from "vitest"

import {
    buildSettlementsSearchParams,
    parseSettlementsSearchParams,
} from "./url-state"
import {
    buildSettlementFilterChips,
    hasAppliedSettlementFilters,
} from "./settlement-list-filters"

describe("settlements url and three-role filters", () => {
    it("round-trips owner/operator/handler independently", () => {
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams(
                "ownerUserIds=u-owner&operatorUserIds=u-op&handlerUserIds=u-review&orgUnitIds=org-a&page=2",
            ),
        )
        expect(parsed.ownerUserIds).toBe("u-owner")
        expect(parsed.operatorUserIds).toBe("u-op")
        expect(parsed.handlerUserIds).toBe("u-review")
        expect(parsed.orgUnitIds).toBe("org-a")
        expect(parsed.includeDescendants).toBe(false)
        const qs = buildSettlementsSearchParams(parsed)
        expect(qs).toContain("ownerUserIds=u-owner")
        expect(qs).toContain("operatorUserIds=u-op")
        expect(qs).toContain("handlerUserIds=u-review")
        expect(qs).not.toContain("company")
    })

    it("builds independent chips and applied-filter state", () => {
        const state = {
            q: undefined,
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            ownerUserIds: "u-owner",
            operatorUserIds: "u-op",
            handlerUserIds: "u-review",
            orgUnitIds: "org-a",
        }
        expect(hasAppliedSettlementFilters(state)).toBe(true)
        const chips = buildSettlementFilterChips(state, [])
        expect(chips.map((chip) => chip.key)).toEqual([
            "ownerUserIds",
            "operatorUserIds",
            "handlerUserIds",
            "orgUnitIds",
        ])
    })
})
