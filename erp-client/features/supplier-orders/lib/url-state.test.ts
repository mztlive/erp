import { describe, expect, it } from "vitest"

import {
    buildSupplierOrdersSearchParams,
    parseSupplierOrdersSearchParams,
    type SupplierOrdersUrlState,
} from "./url-state"

describe("parseSupplierOrdersSearchParams — defaults", () => {
    it("falls back to defaults for an empty query string", () => {
        const state = parseSupplierOrdersSearchParams(new URLSearchParams())

        expect(state.view).toBe("actionable")
        expect(state.page).toBe(1)
        expect(state.pageSize).toBe(50)
        expect(state.section).toBe("overview")
        expect(state.aftersalePending).toBe(false)
        expect(state.q).toBeUndefined()
        expect(state.supplierId).toBeUndefined()
        expect(state.fulfillmentStatuses).toBeUndefined()
        expect(state.preview).toBeUndefined()
        expect(state.sort).toBeUndefined()
        expect(state.dir).toBeUndefined()
    })

    it("falls back to defaults for invalid values", () => {
        const state = parseSupplierOrdersSearchParams(
            new URLSearchParams(
                "view=unknown&page=abc&page=0&pageSize=-3&dir=sideways&section=missing",
            ),
        )

        expect(state.view).toBe("actionable")
        expect(state.page).toBe(1)
        expect(state.pageSize).toBe(50)
        expect(state.dir).toBeUndefined()
        expect(state.section).toBe("overview")
    })
})

describe("parseSupplierOrdersSearchParams — values", () => {
    it("parses valid fields and clamps pageSize to the upper bound", () => {
        const state = parseSupplierOrdersSearchParams(
            new URLSearchParams(
                "view=all&q=%20%20SFO-9%20%20&supplierId=sup_1&page=3&pageSize=1000&dir=asc&sort=updated",
            ),
        )

        expect(state.view).toBe("all")
        // parse 原样读取；trim 只在 build 写回时生效
        expect(state.q).toBe("  SFO-9  ")
        expect(state.supplierId).toBe("sup_1")
        expect(state.page).toBe(3)
        expect(state.pageSize).toBe(100)
        expect(state.dir).toBe("asc")
        expect(state.sort).toBe("updated")
    })

    it("parses status arrays, dropping unknown entries", () => {
        const state = parseSupplierOrdersSearchParams(
            new URLSearchParams(
                "fulfillmentStatus=RESULT_UNKNOWN,BOGUS,EXCEPTION&cancelStatus=CANCELED&refundStatus=PARTIAL",
            ),
        )

        expect(state.fulfillmentStatuses).toEqual([
            "RESULT_UNKNOWN",
            "EXCEPTION",
        ])
        expect(state.cancelStatuses).toEqual(["CANCELED"])
        expect(state.refundStatuses).toEqual(["PARTIAL"])
    })

    it("reads booleans from 0/1 only", () => {
        expect(
            parseSupplierOrdersSearchParams(
                new URLSearchParams("aftersalePending=1"),
            ).aftersalePending,
        ).toBe(true)
        expect(
            parseSupplierOrdersSearchParams(
                new URLSearchParams("aftersalePending=0"),
            ).aftersalePending,
        ).toBe(false)
        expect(
            parseSupplierOrdersSearchParams(
                new URLSearchParams("aftersalePending=yes"),
            ).aftersalePending,
        ).toBe(false)
    })

    it("resolves aliases for preview and sourceId", () => {
        const state = parseSupplierOrdersSearchParams(
            new URLSearchParams("supplierOrderId=so_1&mallOrderId=mo_1"),
        )

        expect(state.preview).toBe("so_1")
        expect(state.sourceId).toBe("mo_1")
    })
})

describe("buildSupplierOrdersSearchParams", () => {
    it("writes only the always-present boolean for a minimal URL", () => {
        const qs = buildSupplierOrdersSearchParams(
            parseSupplierOrdersSearchParams(new URLSearchParams()),
        )
        // boolean 字段只要定义就回写（默认 false → "0"），其余全部省略
        expect(qs).toBe("?aftersalePending=0")
    })

    it("writes non-default values with the canonical keys", () => {
        const qs = buildSupplierOrdersSearchParams({
            view: "all",
            q: "SFO-9",
            supplierId: "sup_1",
            fulfillmentStatuses: ["RESULT_UNKNOWN", "EXCEPTION"],
            cancelStatuses: ["CANCELED"],
            refundStatuses: ["PARTIAL"],
            aftersalePending: true,
            paidFrom: "2026-08-01",
            paidTo: "2026-08-08",
            page: 2,
            pageSize: 20,
            preview: "so_1",
            section: "overview",
            sort: "updated",
            dir: "desc",
        })

        const params = new URLSearchParams(qs)
        expect(params.get("view")).toBe("all")
        expect(params.get("q")).toBe("SFO-9")
        expect(params.get("supplierId")).toBe("sup_1")
        expect(params.get("fulfillmentStatus")).toBe("RESULT_UNKNOWN,EXCEPTION")
        expect(params.get("cancelStatus")).toBe("CANCELED")
        expect(params.get("refundStatus")).toBe("PARTIAL")
        expect(params.get("aftersalePending")).toBe("1")
        expect(params.get("paidFrom")).toBe("2026-08-01")
        expect(params.get("paidTo")).toBe("2026-08-08")
        expect(params.get("page")).toBe("2")
        expect(params.get("pageSize")).toBe("20")
        expect(params.get("preview")).toBe("so_1")
        expect(params.get("sort")).toBe("updated")
        expect(params.get("dir")).toBe("desc")
        // 默认值不回写
        expect(params.get("section")).toBeNull()
    })

    it("round-trips a rich state through parse", () => {
        const original: SupplierOrdersUrlState = {
            view: "recent_completed",
            q: "  A-1 ",
            supplierId: "sup_1",
            fulfillmentStatuses: ["COMPLETED"],
            cancelStatuses: ["NONE"],
            refundStatuses: undefined,
            aftersalePending: false,
            paidFrom: undefined,
            paidTo: undefined,
            page: 4,
            pageSize: 25,
            preview: "so_9",
            section: "items",
            workItemId: "wi_1",
            sort: "identity",
            dir: "asc",
        }
        const rebuilt = parseSupplierOrdersSearchParams(
            new URLSearchParams(buildSupplierOrdersSearchParams(original)),
        )

        expect(rebuilt.view).toBe(original.view)
        expect(rebuilt.q).toBe("A-1")
        expect(rebuilt.supplierId).toBe(original.supplierId)
        expect(rebuilt.fulfillmentStatuses).toEqual(["COMPLETED"])
        expect(rebuilt.cancelStatuses).toEqual(["NONE"])
        expect(rebuilt.page).toBe(4)
        expect(rebuilt.pageSize).toBe(25)
        expect(rebuilt.preview).toBe("so_9")
        expect(rebuilt.section).toBe("items")
        expect(rebuilt.workItemId).toBe("wi_1")
        expect(rebuilt.sort).toBe("identity")
        expect(rebuilt.dir).toBe("asc")
    })
})
