import { describe, expect, it } from "vitest"

import { catalogFixture } from "./fixtures"
import {
    buildCatalogSearchParams,
    buildDetailSearchParams,
    hasUnknownCatalogParams,
    hasUnknownDetailParams,
    matchesCatalogFilters,
    parseCatalogSearchParams,
} from "./url-state"

describe("url state", () => {
    it("parses catalog filters and rejects unknown keys", () => {
        const state = parseCatalogSearchParams(
            new URLSearchParams(
                "policy=PROCESS_REQUIRED&status=HAS_DRAFT&q=销售&page=2",
            ),
        )
        expect(state).toEqual({
            policy: "PROCESS_REQUIRED",
            status: "HAS_DRAFT",
            q: "销售",
            page: 2,
        })
        expect(
            hasUnknownCatalogParams(
                new URLSearchParams("policy=PROCESS_REQUIRED&foo=1"),
            ),
        ).toBe(true)
        expect(
            hasUnknownCatalogParams(
                new URLSearchParams("policy=PROCESS_REQUIRED"),
            ),
        ).toBe(false)
        expect(
            hasUnknownDetailParams(new URLSearchParams("view=draft&hack=1")),
        ).toBe(true)
    })

    it("builds query strings with a single leading question mark", () => {
        expect(buildDetailSearchParams({ view: "draft" })).toBe("?view=draft")
        expect(buildDetailSearchParams({ view: "current" })).toBe("")
        expect(buildDetailSearchParams({ view: "history", version: "2" })).toBe(
            "?view=history&version=2",
        )
        expect(
            buildCatalogSearchParams({
                policy: "ALL",
                status: "ALL",
                q: "",
                page: 1,
            }),
        ).toBe("")
        expect(
            hasUnknownDetailParams(new URLSearchParams("??view=draft")),
        ).toBe(true)
    })

    it("filters catalog rows locally", () => {
        const items = catalogFixture()
        const sales = items.filter((item) =>
            matchesCatalogFilters(item, {
                policy: "PROCESS_REQUIRED",
                status: "ALL",
                q: "销售单",
                page: 1,
            }),
        )
        expect(sales.map((item) => item.document_type)).toContain("sales_order")
        expect(sales.map((item) => item.document_type)).toContain(
            "voucher_sales_order",
        )
    })
})
