import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    resolveSection,
    useSupplierOrderCenterSection,
} from "./use-supplier-order-center-section"

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
    usePathname: () => "/supplier-api/orders/o1",
    useParams: () => ({}),
}))

beforeEach(() => {
    mocks.replace.mockClear()
    mocks.searchParams = new URLSearchParams()
})

describe("resolveSection", () => {
    it("returns overview for missing, empty or unknown values", () => {
        expect(resolveSection(undefined)).toBe("overview")
        expect(resolveSection(null)).toBe("overview")
        expect(resolveSection("")).toBe("overview")
        expect(resolveSection("bogus")).toBe("overview")
    })

    it("returns every registered section unchanged", () => {
        for (const section of [
            "overview",
            "items",
            "fulfillment",
            "aftersales",
            "costs",
            "audit",
        ]) {
            expect(resolveSection(section)).toBe(section)
        }
    })
})

describe("useSupplierOrderCenterSection", () => {
    it("defaults to overview without a section param", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1"),
        )
        expect(result.current.activeSection).toBe("overview")
    })

    it("reads the section from the URL", () => {
        mocks.searchParams = new URLSearchParams("section=audit")
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1"),
        )
        expect(result.current.activeSection).toBe("audit")
    })

    it("prefers the route prop over the URL param", () => {
        mocks.searchParams = new URLSearchParams("section=audit")
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1", "costs"),
        )
        expect(result.current.activeSection).toBe("costs")
    })

    it("writes the section into the URL and keeps other params", () => {
        mocks.searchParams = new URLSearchParams("from=mall-order&sourceId=1")
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1"),
        )
        act(() => {
            result.current.setSection("items")
        })
        expect(mocks.replace).toHaveBeenCalledTimes(1)
        expect(mocks.replace).toHaveBeenCalledWith(
            "/supplier-api/orders/o1?from=mall-order&sourceId=1&section=items",
            { scroll: false },
        )
    })

    it("removes the section param when switching back to overview", () => {
        mocks.searchParams = new URLSearchParams("section=items")
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1"),
        )
        act(() => {
            result.current.setSection("overview")
        })
        expect(mocks.replace).toHaveBeenCalledWith(
            "/supplier-api/orders/o1",
            { scroll: false },
        )
    })

    it("falls back to overview for an unknown URL section", () => {
        mocks.searchParams = new URLSearchParams("section=nope")
        const { result } = renderHook(() =>
            useSupplierOrderCenterSection("o1"),
        )
        expect(result.current.activeSection).toBe("overview")
    })
})
