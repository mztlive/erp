import { describe, expect, it } from "vitest"

import { joinFilterCodes } from "@/features/master-data/api/lists/supplier"

describe("supplier list scope filters", () => {
    it("keeps maintainer and capability owner as independent query params", () => {
        expect(joinFilterCodes(["buyer-a", "buyer-b"])).toBe("buyer-a,buyer-b")
        const query = {
            owner_user_ids: "buyer-a",
            capability_owner_user_ids: "cap-b",
            org_unit_ids: "org-1",
            include_descendants: true,
        }
        expect(query.owner_user_ids).not.toBe(query.capability_owner_user_ids)
        expect(query.include_descendants).toBe(true)
    })
})
