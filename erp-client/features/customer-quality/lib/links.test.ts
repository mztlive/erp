import { describe, expect, it } from "vitest"

import type { CustomerQualityRow } from "../types"
import {
    buildReturnTo,
    customerHref,
    profitLossHref,
    receivablesHref,
    salesOrdersHref,
    withReturnFocus,
} from "./links"

const row: CustomerQualityRow = {
    customerId: "c1",
    customerNo: "NO-001",
    customerName: "示例客户",
    ownerLabels: ["张三"],
    tags: [],
    salesGrossAmount: "1,200.00",
    salesOrderCount: 3,
    voucherShare: "20%",
    nonVoucherShare: "80%",
    costCoveredNetRevenue: "800.00",
    costUncoveredNetRevenue: "200.00",
    costCoverageRate: "80.0%",
    actualProfitLossNet: "100.00",
    marginRate: "12%",
    receivableOpenGross: "500.00",
    overdueGross: "50.00",
    averageCollectionDays: "30",
    exceptionCounts: { return: 1 },
    firstBusinessAt: "2026-01-01T00:00:00+08:00",
    latestBusinessAt: "2026-07-01T00:00:00+08:00",
    scaleTierCode: "s1",
    profitTierCode: "p1",
    riskTierCode: "r1",
    cardFundsReviewInsufficient: false,
    allowedDrilldowns: ["W03"],
}

describe("buildReturnTo", () => {
    it("keeps the query string when present", () => {
        expect(
            buildReturnTo(
                "/analytics/customer-quality",
                new URLSearchParams("page=2&q=abc"),
            ),
        ).toBe("/analytics/customer-quality?page=2&q=abc")
    })

    it("returns the bare pathname for empty params", () => {
        expect(
            buildReturnTo(
                "/analytics/customer-quality",
                new URLSearchParams(),
            ),
        ).toBe("/analytics/customer-quality")
    })
})

describe("withReturnFocus", () => {
    it("adds the focus customer id to the return URL", () => {
        expect(
            withReturnFocus("/analytics/customer-quality?page=2", "c1"),
        ).toBe("/analytics/customer-quality?page=2&focusCustomerId=c1")
    })

    it("adds the focus metric when provided", () => {
        expect(
            withReturnFocus(
                "/analytics/customer-quality",
                "c1",
                "overdueGross",
            ),
        ).toBe(
            "/analytics/customer-quality?focusCustomerId=c1&focusMetric=overdueGross",
        )
    })
})

describe("customerHref", () => {
    it("links to the customer center with source and return info", () => {
        const href = customerHref(
            "c1",
            "示例客户",
            "/analytics/customer-quality?page=1",
        )
        const params = new URLSearchParams(href.split("?")[1])
        expect(href.startsWith("/sales/customers/c1?")).toBe(true)
        expect(params.get("from")).toBe("W15")
        expect(params.get("customerName")).toBe("示例客户")
        expect(params.get("returnTo")).toBe(
            "/analytics/customer-quality?page=1",
        )
    })
})

describe("salesOrdersHref", () => {
    it("encodes customer, period, return and nature params", () => {
        const href = salesOrdersHref(
            row,
            { from: "2026-01-01", to: "2026-06-30" },
            "/analytics/customer-quality",
            "GOODS_SERVICE",
        )
        const params = new URLSearchParams(href.split("?")[1])
        expect(href.startsWith("/sales/orders?")).toBe(true)
        expect(params.get("search")).toBe("示例客户")
        expect(params.get("customerId")).toBe("c1")
        expect(params.get("from")).toBe("W15")
        expect(params.get("periodFrom")).toBe("2026-01-01")
        expect(params.get("periodTo")).toBe("2026-06-30")
        expect(params.get("returnTo")).toBe("/analytics/customer-quality")
        expect(params.get("nature")).toBe("physical_service")
    })

    it("maps VOUCHER to card_voucher and omits nature without a filter", () => {
        const withVoucher = new URLSearchParams(
            salesOrdersHref(
                row,
                { from: "a", to: "b" },
                "/x",
                "VOUCHER",
            ).split("?")[1],
        )
        expect(withVoucher.get("nature")).toBe("card_voucher")

        const withoutType = new URLSearchParams(
            salesOrdersHref(row, { from: "a", to: "b" }, "/x").split("?")[1],
        )
        expect(withoutType.has("nature")).toBe(false)
    })
})

describe("receivablesHref", () => {
    it("links to the overdue receivable view with return info", () => {
        const href = receivablesHref(
            row,
            { from: "2026-01-01", to: "2026-06-30" },
            "/analytics/customer-quality",
        )
        const params = new URLSearchParams(href.split("?")[1])
        expect(href.startsWith("/finance/customer-accounts?")).toBe(true)
        expect(params.get("view")).toBe("receivable")
        expect(params.get("due")).toBe("overdue")
        expect(params.get("customerId")).toBe("c1")
        expect(params.get("from")).toBe("W15")
        expect(params.get("periodFrom")).toBe("2026-01-01")
        expect(params.get("periodTo")).toBe("2026-06-30")
    })
})

describe("profitLossHref", () => {
    it("links to the profit-loss analysis with period and return info", () => {
        const href = profitLossHref(
            row,
            { from: "2026-01-01", to: "2026-06-30" },
            "/analytics/customer-quality",
        )
        const params = new URLSearchParams(href.split("?")[1])
        expect(href.startsWith("/analytics/profit-loss?")).toBe(true)
        expect(params.get("customerId")).toBe("c1")
        expect(params.get("from")).toBe("2026-01-01")
        expect(params.get("to")).toBe("2026-06-30")
        expect(params.get("source")).toBe("W15")
        expect(params.get("returnTo")).toBe("/analytics/customer-quality")
    })
})
