import { describe, expect, it } from "vitest"

import {
    buildSupplierOfferingsSearchParams,
    parseSupplierOfferingsSearchParams,
} from "./url-state"

describe("supplier offerings url state", () => {
    it("round-trips maintainer and procurement owner filters", () => {
        const query = buildSupplierOfferingsSearchParams({
            page: 1,
            ownerUserIds: "user-1,user-2",
            procurementOwnerUserIds: "buyer-1",
            orgUnitIds: "org-1",
            includeDescendants: true,
        })
        const parsed = parseSupplierOfferingsSearchParams(
            new URLSearchParams(query.startsWith("?") ? query.slice(1) : query),
        )
        expect(parsed.ownerUserIds).toBe("user-1,user-2")
        expect(parsed.procurementOwnerUserIds).toBe("buyer-1")
        expect(parsed.orgUnitIds).toBe("org-1")
        expect(parsed.includeDescendants).toBe(true)
    })
})
