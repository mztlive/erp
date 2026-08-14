import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useListUrl, useSearchDraft } from "./use-list-url"

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/master-data/warehouses",
    useParams: () => ({}),
}))

function lastReplaceUrl(): string {
    const call = navMocks.replace.mock.calls.at(-1)
    return String(call?.[0] ?? "")
}

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.push.mockClear()
    navMocks.replace.mockClear()
})

describe("useListUrl", () => {
    it("falls back to defaults on an empty URL", () => {
        const { result } = renderHook(() => useListUrl())
        expect(result.current.q).toBe("")
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 20,
        })
    })

    it("reads q from the URL", () => {
        navMocks.searchParams = new URLSearchParams("q=仓库A")
        const { result } = renderHook(() => useListUrl())
        expect(result.current.q).toBe("仓库A")
    })

    it("parses the page param into a zero-based index", () => {
        navMocks.searchParams = new URLSearchParams("page=3")
        const { result } = renderHook(() => useListUrl())
        expect(result.current.pagination.pageIndex).toBe(2)
    })

    it("clamps invalid page params to the first page", () => {
        navMocks.searchParams = new URLSearchParams("page=abc")
        const first = renderHook(() => useListUrl())
        expect(first.result.current.pagination.pageIndex).toBe(0)

        navMocks.searchParams = new URLSearchParams("page=0")
        const second = renderHook(() => useListUrl())
        expect(second.result.current.pagination.pageIndex).toBe(0)

        navMocks.searchParams = new URLSearchParams("page=-2")
        const third = renderHook(() => useListUrl())
        expect(third.result.current.pagination.pageIndex).toBe(0)
    })

    it("patches params with replace, removing null entries", () => {
        navMocks.searchParams = new URLSearchParams("q=旧词&page=2")
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.patchUrl({ q: "新词", page: null })
        })

        expect(lastReplaceUrl()).toBe(
            "/master-data/warehouses?q=%E6%96%B0%E8%AF%8D",
        )
        expect(navMocks.replace.mock.calls.at(-1)?.[1]).toEqual({
            scroll: false,
        })
    })

    it("strips the query string entirely when no params remain", () => {
        navMocks.searchParams = new URLSearchParams("q=x")
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.patchUrl({ q: null })
        })

        expect(lastReplaceUrl()).toBe("/master-data/warehouses")
    })

    it("keeps untouched params when patching", () => {
        navMocks.searchParams = new URLSearchParams("lifecycleStatus=enabled")
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.patchUrl({ q: "abc" })
        })

        const url = lastReplaceUrl()
        expect(url).toContain("lifecycleStatus=enabled")
        expect(url).toContain("q=abc")
    })

    it("changes pagination and writes the page param", () => {
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.changePagination({ pageIndex: 1, pageSize: 20 })
        })

        expect(result.current.pagination.pageIndex).toBe(1)
        expect(lastReplaceUrl()).toBe("/master-data/warehouses?page=2")
    })

    it("removes the page param when returning to the first page", () => {
        navMocks.searchParams = new URLSearchParams("page=2")
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.changePagination({ pageIndex: 0, pageSize: 20 })
        })

        expect(result.current.pagination.pageIndex).toBe(0)
        expect(lastReplaceUrl()).toBe("/master-data/warehouses")
    })

    it("resetPagination returns to page 0 without touching the URL", () => {
        const { result } = renderHook(() => useListUrl())

        act(() => {
            result.current.setPagination({ pageIndex: 3, pageSize: 20 })
        })
        act(() => {
            result.current.resetPagination()
        })

        expect(result.current.pagination.pageIndex).toBe(0)
        expect(navMocks.replace).not.toHaveBeenCalled()
    })
})

describe("useSearchDraft", () => {
    it("starts from the URL value", () => {
        const input = { current: null }
        const { result } = renderHook(() => useSearchDraft("abc", input))
        expect(result.current.searchDraft).toBe("abc")
    })

    it("syncs from the URL when the search input is not focused", () => {
        const input = { current: null }
        const { result, rerender } = renderHook(
            ({ q }: { q: string }) => useSearchDraft(q, input),
            { initialProps: { q: "one" } },
        )
        expect(result.current.searchDraft).toBe("one")
        rerender({ q: "two" })
        expect(result.current.searchDraft).toBe("two")
    })

    it("protects an in-progress draft while the search input is focused", () => {
        const inputElement = document.createElement("input")
        document.body.appendChild(inputElement)
        inputElement.focus()
        const input = { current: inputElement }

        const { result, rerender } = renderHook(
            ({ q }: { q: string }) => useSearchDraft(q, input),
            { initialProps: { q: "one" } },
        )
        act(() => {
            result.current.setSearchDraft("draft-in-progress")
        })
        rerender({ q: "two" })
        expect(result.current.searchDraft).toBe("draft-in-progress")

        inputElement.blur()
        document.body.removeChild(inputElement)
    })
})
