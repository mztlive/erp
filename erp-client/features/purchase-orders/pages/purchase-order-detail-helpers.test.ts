import { describe, it, expect } from "vitest"

import {
    buildPurchaseOrderDetailNavItems,
    resolvePurchaseOrderDetailMode,
    resolvePurchaseOrderDetailSection,
} from "./purchase-order-detail-helpers"

describe("resolvePurchaseOrderDetailSection", () => {
    it.each(["lines", "fulfillment", "payable", "changes", "audit"])(
        "returns %s verbatim",
        (section) => {
            expect(resolvePurchaseOrderDetailSection(section)).toBe(section)
        },
    )

    it("falls back to overview for undefined or unknown sections", () => {
        expect(resolvePurchaseOrderDetailSection()).toBe("overview")
        expect(resolvePurchaseOrderDetailSection("bogus")).toBe("overview")
    })
})

describe("resolvePurchaseOrderDetailMode", () => {
    it.each(["edit", "review"])("returns %s verbatim", (mode) => {
        expect(resolvePurchaseOrderDetailMode(mode)).toBe(mode)
    })

    it("falls back to view for undefined or unknown modes", () => {
        expect(resolvePurchaseOrderDetailMode()).toBe("view")
        expect(resolvePurchaseOrderDetailMode("bogus")).toBe("view")
    })
})

describe("buildPurchaseOrderDetailNavItems", () => {
    it("keeps the overview href clean and appends sections", () => {
        const items = buildPurchaseOrderDetailNavItems(
            "/procurement/orders/po-1",
            "view",
        )
        expect(items.map((item) => item.id)).toEqual([
            "overview",
            "lines",
            "fulfillment",
            "payable",
            "changes",
            "audit",
        ])
        expect(items[0].href).toBe("/procurement/orders/po-1")
        expect(items[1].href).toBe("/procurement/orders/po-1?section=lines")
        expect(items[5].href).toBe("/procurement/orders/po-1?section=audit")
    })

    it("carries the mode on section links in non-view modes", () => {
        const items = buildPurchaseOrderDetailNavItems(
            "/procurement/orders/po-1",
            "edit",
        )
        expect(items[1].href).toBe(
            "/procurement/orders/po-1?section=lines&mode=edit",
        )
        expect(items[0].href).toBe("/procurement/orders/po-1")
    })
})
