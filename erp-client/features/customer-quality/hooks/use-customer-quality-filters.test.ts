import { beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import { useCustomerQualityFilters } from "./use-customer-quality-filters"

const mocks = vi.hoisted(() => ({
    patchUrl: vi.fn(),
}))

function renderFilters(overrides: {
    qParam?: string
    fundsReview?: "all" | "reviewed_only"
    businessType?: "VOUCHER" | "GOODS_SERVICE" | undefined
    customerId?: string
    customerName?: string
} = {}) {
    return renderHook(() =>
        useCustomerQualityFilters({
            qParam: overrides.qParam ?? "",
            fundsReview: overrides.fundsReview ?? "all",
            businessType: overrides.businessType,
            customerId: overrides.customerId,
            customerName: overrides.customerName,
            patchUrl: mocks.patchUrl,
        }),
    )
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useCustomerQualityFilters", () => {
    it("derives drafts from the URL and opens the panel on structured deep links", () => {
        const { result } = renderFilters({
            qParam: "abc",
            fundsReview: "reviewed_only",
            businessType: "VOUCHER",
        })

        expect(result.current.searchDraft).toBe("abc")
        expect(result.current.fundsReviewDraft).toBe("reviewed_only")
        expect(result.current.businessTypeDraft).toBe("VOUCHER")
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
    })

    it("keeps the panel closed without structured filters", () => {
        const { result } = renderFilters()
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
    })

    it("applyFilters writes q and structured params once and closes the panel", () => {
        const { result } = renderFilters()
        act(() => {
            result.current.setSearchDraft("  abc  ")
            result.current.setFundsReviewDraft("reviewed_only")
            result.current.setBusinessTypeDraft("GOODS_SERVICE")
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(mocks.patchUrl).toHaveBeenCalledTimes(1)
        expect(mocks.patchUrl).toHaveBeenCalledWith({
            q: "abc",
            fundsReview: "reviewed_only",
            businessType: "GOODS_SERVICE",
        })
        expect(result.current.panelOpen).toBe(false)
    })

    it("omits defaults from the URL patch", () => {
        const { result } = renderFilters()
        act(() => {
            result.current.applyFilters()
        })

        expect(mocks.patchUrl).toHaveBeenCalledWith({
            q: null,
            fundsReview: null,
            businessType: null,
        })
    })

    it("clearAllFilters resets drafts and the panel and clears only filter params", () => {
        const { result } = renderFilters({
            qParam: "abc",
            fundsReview: "reviewed_only",
            customerId: "c1",
        })
        act(() => {
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.clearAllFilters()
        })

        expect(result.current.searchDraft).toBe("")
        expect(result.current.fundsReviewDraft).toBe("all")
        expect(result.current.businessTypeDraft).toBe("all")
        expect(result.current.panelOpen).toBe(false)
        expect(mocks.patchUrl).toHaveBeenCalledWith({
            q: null,
            fundsReview: null,
            businessType: null,
            customerId: null,
            scaleTag: null,
            profitTag: null,
            riskTag: null,
            chartDimension: null,
            chartCode: null,
            focusMetric: null,
        })
        // 排序、期间等非筛选参数不在清除范围内
        expect(
            Object.keys(mocks.patchUrl.mock.calls[0]![0]),
        ).not.toContain("sort")
        expect(
            Object.keys(mocks.patchUrl.mock.calls[0]![0]),
        ).not.toContain("from")
    })

    it("resetMoreFilters clears only structured conditions, keeps q, and keeps the panel open", () => {
        const { result } = renderFilters({
            qParam: "abc",
            fundsReview: "reviewed_only",
            businessType: "VOUCHER",
        })
        act(() => {
            result.current.setFundsReviewDraft("reviewed_only")
            result.current.setBusinessTypeDraft("VOUCHER")
        })
        act(() => {
            result.current.resetMoreFilters()
        })

        expect(result.current.fundsReviewDraft).toBe("all")
        expect(result.current.businessTypeDraft).toBe("all")
        expect(result.current.searchDraft).toBe("abc")
        expect(result.current.panelOpen).toBe(true)
        expect(mocks.patchUrl).toHaveBeenCalledWith({
            fundsReview: null,
            businessType: null,
        })
    })

    it("removeFilter removes a single condition and its draft", () => {
        const { result } = renderFilters({
            qParam: "abc",
            fundsReview: "reviewed_only",
            businessType: "VOUCHER",
            customerId: "c1",
        })

        act(() => {
            result.current.removeFilter("q")
        })
        expect(result.current.searchDraft).toBe("")
        expect(mocks.patchUrl).toHaveBeenLastCalledWith({ q: null })

        act(() => {
            result.current.removeFilter("fundsReview")
        })
        expect(result.current.fundsReviewDraft).toBe("all")

        act(() => {
            result.current.removeFilter("businessType")
        })
        expect(result.current.businessTypeDraft).toBe("all")

        act(() => {
            result.current.removeFilter("customerId")
        })
        expect(mocks.patchUrl).toHaveBeenLastCalledWith({
            customerId: null,
        })

        act(() => {
            result.current.removeFilter("chart")
        })
        expect(mocks.patchUrl).toHaveBeenLastCalledWith({
            chartDimension: null,
            chartCode: null,
            scaleTag: null,
            profitTag: null,
            riskTag: null,
        })
    })

    it("URL backfill syncs structured drafts without reopening the panel", () => {
        const { result, rerender } = renderHook(
            ({ businessType }: { businessType?: "VOUCHER" }) =>
                useCustomerQualityFilters({
                    qParam: "",
                    fundsReview: "all",
                    businessType,
                    patchUrl: mocks.patchUrl,
                }),
            { initialProps: { businessType: undefined as "VOUCHER" | undefined } },
        )

        expect(result.current.panelOpen).toBe(false)
        rerender({ businessType: "VOUCHER" })
        expect(result.current.businessTypeDraft).toBe("VOUCHER")
        // 回填不得强制展开面板
        expect(result.current.panelOpen).toBe(false)
    })

    it("builds chips for every applied condition including the locked customer", () => {
        const { result } = renderFilters({
            qParam: "abc",
            fundsReview: "reviewed_only",
            businessType: "VOUCHER",
            customerId: "c1",
            customerName: "华东商贸",
        })

        expect(result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：abc" },
            { key: "fundsReview", label: "票款口径：仅已复核卡券票款" },
            { key: "businessType", label: "业务性质：卡券" },
            { key: "customerId", label: "客户：华东商贸" },
        ])
    })

    it("falls back to a generic label for the locked customer before data loads", () => {
        const { result } = renderFilters({ customerId: "c1" })
        expect(result.current.appliedChips).toEqual([
            { key: "customerId", label: "客户：已定位客户" },
        ])
    })
})
