import { beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import { useCustomerQualityNavigationState } from "./use-customer-quality-navigation-state"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: mocks.push,
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => "/analytics/customer-quality",
    useParams: () => ({}),
}))

beforeEach(() => {
    vi.clearAllMocks()
    mocks.searchParams = new URLSearchParams()
})

describe("useCustomerQualityNavigationState", () => {
    it("derives the initial page and sort from the URL", () => {
        mocks.searchParams = new URLSearchParams(
            "page=3&sort=overdueGross:desc",
        )
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        expect(result.current.pagination).toEqual({
            pageIndex: 2,
            pageSize: 20,
        })
        expect(result.current.sort).toBe("overdueGross:desc")
        expect(result.current.tableSorting).toEqual([
            { id: "overdueGross", desc: true },
        ])
    })

    it("falls back to page 1 and default sort for missing or invalid params", () => {
        mocks.searchParams = new URLSearchParams("page=abc&sort=whatever")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        expect(result.current.pagination.pageIndex).toBe(0)
        expect(result.current.sort).toBe("whatever")
        expect(result.current.tableSorting).toEqual([
            { id: "whatever", desc: false },
        ])
    })

    it("syncs pagination when the URL page param changes", () => {
        const { result, rerender } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )
        expect(result.current.pagination.pageIndex).toBe(0)

        mocks.searchParams = new URLSearchParams("page=5")
        rerender()
        expect(result.current.pagination.pageIndex).toBe(4)
    })

    it("patchUrl writes the patch via replace, drops page and resets pagination", () => {
        mocks.searchParams = new URLSearchParams("page=3&fundsReview=all")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )
        expect(result.current.pagination.pageIndex).toBe(2)

        act(() => {
            result.current.patchUrl({ fundsReview: "reviewed_only" })
        })

        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?fundsReview=reviewed_only",
            { scroll: false },
        )
        expect(mocks.push).not.toHaveBeenCalled()
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("patchUrl deletes keys set to null or empty and honors the options", () => {
        mocks.searchParams = new URLSearchParams("q=abc&businessType=VOUCHER")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.patchUrl(
                { q: null, businessType: "" },
                { replace: false },
            )
        })

        expect(mocks.push).toHaveBeenCalledWith(
            "/analytics/customer-quality",
            { scroll: false },
        )
    })

    it("resetPage returns pagination to the first page", () => {
        mocks.searchParams = new URLSearchParams("page=3")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.resetPage()
        })

        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("handlePaginationChange writes the page param into the URL", () => {
        mocks.searchParams = new URLSearchParams("q=abc")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 20,
            })
        })

        expect(result.current.pagination.pageIndex).toBe(2)
        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?q=abc&page=3",
        )
    })

    it("handlePaginationChange omits page for the first page", () => {
        mocks.searchParams = new URLSearchParams("q=abc&page=2")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 0,
                pageSize: 20,
            })
        })

        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?q=abc",
        )
    })

    it("handleTableSortingChange writes the sort param and resets the page", () => {
        mocks.searchParams = new URLSearchParams("page=4")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.handleTableSortingChange([
                { id: "actualProfitLossNet", desc: true },
            ])
        })

        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?sort=actualProfitLossNet%3Adesc",
            { scroll: false },
        )
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("handleTableSortingChange restores the default sort when cleared", () => {
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        act(() => {
            result.current.handleTableSortingChange([])
        })

        expect(mocks.replace).toHaveBeenCalledWith(
            "/analytics/customer-quality?sort=salesGrossAmount%3Adesc",
            { scroll: false },
        )
    })

    it("builds the returnTo from the current path and params", () => {
        mocks.searchParams = new URLSearchParams("page=2&q=abc")
        const { result } = renderHook(() =>
            useCustomerQualityNavigationState(),
        )

        expect(result.current.returnTo).toBe(
            "/analytics/customer-quality?page=2&q=abc",
        )
    })
})
