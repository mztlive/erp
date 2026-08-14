import { describe, expect, it } from "vitest"

import {
    buildSupplierOfferingsSearchParams,
    parseSupplierOfferingsSearchParams,
} from "./url-state"

describe("parseSupplierOfferingsSearchParams", () => {
    it("returns defaults for an empty query string", () => {
        const state = parseSupplierOfferingsSearchParams(new URLSearchParams())

        expect(state.page).toBe(1)
        expect(state.q).toBeUndefined()
        expect(state.skuId).toBeUndefined()
        expect(state.skuNo).toBeUndefined()
        expect(state.productNo).toBeUndefined()
        expect(state.supplierId).toBeUndefined()
        expect(state.status).toBeUndefined()
        expect(state.sourceType).toBeUndefined()
        expect(state.availabilityStatus).toBeUndefined()
        expect(state.returnTo).toBeUndefined()
        expect(state.workItemId).toBeUndefined()
    })

    it("parses all supported fields including snake_case aliases", () => {
        const state = parseSupplierOfferingsSearchParams(
            new URLSearchParams(
                "q=abc&skuId=sku_1&sku_no=SKU-001&product_no=P-1001&supplierId=sup_1" +
                    "&status=ACTIVE&sourceType=EXCEL&availabilityStatus=STALE" +
                    "&page=3&returnTo=/products&workItemId=wi_1&queueContextId=qc_1&from=W02",
            ),
        )

        expect(state).toEqual({
            q: "abc",
            skuId: "sku_1",
            skuNo: "SKU-001",
            productNo: "P-1001",
            supplierId: "sup_1",
            status: "ACTIVE",
            sourceType: "EXCEL",
            availabilityStatus: "STALE",
            page: 3,
            returnTo: "/products",
            workItemId: "wi_1",
            queueContextId: "qc_1",
            from: "W02",
        })
    })

    it("keeps raw string values on parse and drops invalid enum values", () => {
        const state = parseSupplierOfferingsSearchParams(
            new URLSearchParams("q=%20abc%20&status=UNKNOWN&sourceType=MANUAL"),
        )

        // 解析不做清洗；写入时由 build 统一 trim（契约见 lib/url-state）。
        expect(state.q).toBe(" abc ")
        expect(state.status).toBeUndefined()
        expect(state.sourceType).toBe("MANUAL")
    })

    it("trims and drops whitespace-only strings when building", () => {
        expect(
            buildSupplierOfferingsSearchParams({
                q: "  abc  ",
                page: 1,
            }),
        ).toBe("?q=abc")
        expect(
            buildSupplierOfferingsSearchParams({
                q: "   ",
                page: 1,
            }),
        ).toBe("")
    })

    it("clamps and floors the page number", () => {
        expect(
            parseSupplierOfferingsSearchParams(
                new URLSearchParams("page=0"),
            ).page,
        ).toBe(1)
        expect(
            parseSupplierOfferingsSearchParams(
                new URLSearchParams("page=abc"),
            ).page,
        ).toBe(1)
        expect(
            parseSupplierOfferingsSearchParams(
                new URLSearchParams("page=2.7"),
            ).page,
        ).toBe(2)
    })
})

describe("buildSupplierOfferingsSearchParams", () => {
    it("produces an empty string for the default state", () => {
        expect(
            buildSupplierOfferingsSearchParams({
                page: 1,
            }),
        ).toBe("")
    })

    it("skips empty and undefined values and keeps only non-default fields", () => {
        expect(
            buildSupplierOfferingsSearchParams({
                q: "abc",
                skuId: "sku_1",
                skuNo: "",
                productNo: undefined,
                supplierId: undefined,
                status: undefined,
                sourceType: undefined,
                availabilityStatus: undefined,
                page: 1,
                returnTo: "/products",
            }),
        ).toBe("?q=abc&skuId=sku_1&returnTo=%2Fproducts")
    })

    it("writes the page only when it differs from the default", () => {
        expect(
            buildSupplierOfferingsSearchParams({
                page: 4,
            }),
        ).toBe("?page=4")
    })

    it("round-trips a full state through parse", () => {
        const state = {
            q: "abc",
            skuId: "sku_1",
            skuNo: "SKU-001",
            productNo: "P-1001",
            supplierId: "sup_1",
            status: "PAUSED" as const,
            sourceType: "API" as const,
            availabilityStatus: "UNAVAILABLE" as const,
            page: 2,
            returnTo: "/products",
            workItemId: "wi_1",
            queueContextId: "qc_1",
            from: "W02" as const,
        }

        expect(
            parseSupplierOfferingsSearchParams(
                new URLSearchParams(buildSupplierOfferingsSearchParams(state)),
            ),
        ).toEqual(state)
    })
})
