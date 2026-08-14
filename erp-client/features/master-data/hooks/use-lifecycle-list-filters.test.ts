import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useLifecycleListFilters } from "./use-lifecycle-list-filters"

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

function lastReplaceParams(): URLSearchParams {
    const url = lastReplaceUrl()
    const idx = url.indexOf("?")
    return new URLSearchParams(idx >= 0 ? url.slice(idx + 1) : "")
}

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.push.mockClear()
    navMocks.replace.mockClear()
})

/** 稳定的 ref 对象：内联字面量会在每次渲染时触发草稿同步副作用。 */
const searchInputRef = { current: null as HTMLInputElement | null }

describe("useLifecycleListFilters", () => {
    it("falls back to defaults on an empty URL", () => {
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        expect(result.current.q).toBe("")
        expect(result.current.lifecycleStatus).toBe("all")
        expect(result.current.revisionTiming).toBe("all")
        expect(result.current.metricKey).toBe("all")
        expect(result.current.hasStructuredListFilters).toBe(false)
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 20,
        })
    })

    it("parses lifecycle, revision timing, metric and q from the URL", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=仓库A&lifecycleStatus=enabled&revisionTiming=future&metricKey=enabled",
        )
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        expect(result.current.q).toBe("仓库A")
        expect(result.current.lifecycleStatus).toBe("enabled")
        expect(result.current.revisionTiming).toBe("future")
        expect(result.current.metricKey).toBe("enabled")
        expect(result.current.hasStructuredListFilters).toBe(true)
        expect(result.current.filterPanelOpen).toBe(true)
    })

    it("treats invalid enum values as defaults", () => {
        navMocks.searchParams = new URLSearchParams(
            "lifecycleStatus=bogus&revisionTiming=bogus",
        )
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        expect(result.current.lifecycleStatus).toBe("all")
        expect(result.current.revisionTiming).toBe("all")
    })

    it("commits the search draft into the URL and resets pagination", () => {
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("  新词  ")
        })
        act(() => {
            result.current.commitSearch()
        })

        const params = lastReplaceParams()
        expect(params.get("q")).toBe("新词")
        expect(params.has("page")).toBe(false)
    })

    it("does not touch the URL when the committed search is unchanged", () => {
        navMocks.searchParams = new URLSearchParams("q=abc")
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("abc")
        })
        act(() => {
            result.current.commitSearch()
        })

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("writes lifecycle and metric together when the lifecycle changes", () => {
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.changeLifecycle("enabled")
        })

        const params = lastReplaceParams()
        expect(params.get("lifecycleStatus")).toBe("enabled")
        expect(params.get("metricKey")).toBe("enabled")
        expect(params.has("page")).toBe(false)
    })

    it("clears lifecycle and metric from the URL when reset to all", () => {
        navMocks.searchParams = new URLSearchParams("lifecycleStatus=enabled")
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.changeLifecycle("all")
        })

        const params = lastReplaceParams()
        expect(params.has("lifecycleStatus")).toBe(false)
        expect(params.has("metricKey")).toBe(false)
    })

    it("applies the draft filters into the URL", () => {
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("仓库")
            result.current.setLifecycleStatusDraft("enabled")
            result.current.setRevisionTimingDraft("future")
        })
        act(() => {
            result.current.applyListFilters()
        })

        const params = lastReplaceParams()
        expect(params.get("q")).toBe("仓库")
        expect(params.get("lifecycleStatus")).toBe("enabled")
        expect(params.get("metricKey")).toBe("enabled")
        expect(params.get("revisionTiming")).toBe("future")
        expect(params.has("page")).toBe(false)
    })

    it("clears every filter on clearAllFilters", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=abc&lifecycleStatus=enabled&metricKey=enabled&revisionTiming=future",
        )
        const { result } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        act(() => {
            result.current.clearAllFilters()
        })

        const params = lastReplaceParams()
        expect(params.toString()).toBe("")
        expect(result.current.searchDraft).toBe("")
        expect(result.current.lifecycleStatusDraft).toBe("all")
        expect(result.current.revisionTimingDraft).toBe("all")
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("syncs drafts and the panel from URL changes", () => {
        const { result, rerender } = renderHook(() =>
            useLifecycleListFilters(searchInputRef),
        )
        expect(result.current.filterPanelOpen).toBe(false)

        navMocks.searchParams = new URLSearchParams(
            "lifecycleStatus=disabled&revisionTiming=current",
        )
        rerender()

        expect(result.current.lifecycleStatus).toBe("disabled")
        expect(result.current.lifecycleStatusDraft).toBe("disabled")
        expect(result.current.revisionTimingDraft).toBe("current")
        expect(result.current.filterPanelOpen).toBe(true)
    })
})
