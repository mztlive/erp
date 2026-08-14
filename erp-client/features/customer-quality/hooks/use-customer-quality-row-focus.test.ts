import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import type { CustomerQualityView } from "../types"
import { useCustomerQualityRowFocus } from "./use-customer-quality-row-focus"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => "/analytics/customer-quality",
    useParams: () => ({}),
}))

let rafCallbacks: Array<() => void> = []

function flushRaf() {
    act(() => {
        // 两层嵌套 rAF：每层回调都可能再排一个新的
        for (let i = 0; i < 2; i++) {
            const pending = rafCallbacks.splice(0)
            pending.forEach((cb) => cb())
        }
    })
}

beforeEach(() => {
    vi.clearAllMocks()
    mocks.searchParams = new URLSearchParams()
    rafCallbacks = []
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
        rafCallbacks.push(() => cb(Date.now()))
        return rafCallbacks.length
    })
    vi.stubGlobal("cancelAnimationFrame", () => {})
    document.body.innerHTML = ""
})

afterEach(() => {
    vi.unstubAllGlobals()
})

const view: CustomerQualityView = {
    scope: { id: "scope:team:sales-east", label: "华东", permissionVersion: "v1" },
    period: {
        from: "2026-01-01",
        to: "2026-06-30",
        basis: "BUSINESS_DATE",
        timezone: "Asia/Shanghai",
        selectionSource: "EXPLICIT",
    },
    freshness: {
        projectedAt: "2026-07-01T10:00:00+08:00",
        sourceWatermark: "outbox:cq:2026-07-01T10:00:00+08:00",
        state: "fresh",
    },
    coverage: {
        cardFundsReviewRate: "8/10",
        cardFundsReviewPercent: 80,
        reviewedVoucherOrderCount: 8,
        requiredVoucherOrderCount: 10,
        cardFundsState: "partial",
        costCoveredNetRevenue: "800.00",
        costUncoveredNetRevenue: "200.00",
        costCoverageRate: "80.0%",
        costCoveragePercent: 80,
        costCoverageState: "partial",
        costBasis: "ACTUAL",
    },
    metrics: [],
    dimensions: [],
    customers: { items: [], total: 0, filteredTotal: 0 },
    filterSummary: "全部",
    canExport: true,
    tagRuleCatalog: {
        scale: { ruleVersion: "v1", explanation: "e", labels: {} },
        profit: { ruleVersion: "v1", explanation: "e", labels: {} },
        risk: { ruleVersion: "v1", explanation: "e", labels: {} },
    },
}

describe("useCustomerQualityRowFocus", () => {
    it("clears the focus params from the URL after restoring", () => {
        mocks.searchParams = new URLSearchParams(
            "focusCustomerId=c1&focusMetric=overdueGross&page=1",
        )
        const scrollToTableTop = vi.fn()
        renderHook(() =>
            useCustomerQualityRowFocus({
                focusCustomerId: "c1",
                focusMetric: "overdueGross",
                data: view,
                scrollToTableTop,
            }),
        )

        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?page=1",
        )

        // 目标不在 DOM 中时降级滚动到明细表顶部
        flushRaf()
        expect(scrollToTableTop).toHaveBeenCalledTimes(1)
    })

    it("focuses the matching row or metric element when present", () => {
        mocks.searchParams = new URLSearchParams(
            "focusCustomerId=c1&focusMetric=overdueGross&page=1",
        )
        const target = document.createElement("a")
        target.setAttribute("data-customer-id", "c1")
        target.setAttribute("data-focus-metric", "overdueGross")
        document.body.appendChild(target)
        const focusSpy = vi.spyOn(target, "focus")
        const scrollToTableTop = vi.fn()

        renderHook(() =>
            useCustomerQualityRowFocus({
                focusCustomerId: "c1",
                focusMetric: "overdueGross",
                data: view,
                scrollToTableTop,
            }),
        )

        flushRaf()
        expect(focusSpy).toHaveBeenCalledTimes(1)
        expect(scrollToTableTop).not.toHaveBeenCalled()
        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?page=1",
        )
    })

    it("falls back to the customer row anchor without a focus metric", () => {
        mocks.searchParams = new URLSearchParams("focusCustomerId=c1&page=1")
        const row = document.createElement("div")
        row.setAttribute("data-customer-row", "c1")
        document.body.appendChild(row)
        const focusSpy = vi.spyOn(row, "focus")
        const scrollToTableTop = vi.fn()

        renderHook(() =>
            useCustomerQualityRowFocus({
                focusCustomerId: "c1",
                data: view,
                scrollToTableTop,
            }),
        )

        flushRaf()
        expect(focusSpy).toHaveBeenCalledTimes(1)
        expect(scrollToTableTop).not.toHaveBeenCalled()
    })

    it("does nothing when there is no focus customer id", () => {
        mocks.searchParams = new URLSearchParams("page=1")
        renderHook(() =>
            useCustomerQualityRowFocus({
                data: view,
                scrollToTableTop: vi.fn(),
            }),
        )

        expect(mocks.replace).not.toHaveBeenCalled()
    })
})
