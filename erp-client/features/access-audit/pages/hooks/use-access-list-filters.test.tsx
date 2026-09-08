import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import { useAccessListFilters } from "./use-access-list-filters"

const url = vi.hoisted(() => ({ searchParams: new URLSearchParams() }))
vi.mock("./use-access-url-state", () => ({ useAccessUrlState: () => url }))

afterEach(cleanup)
beforeEach(() => {
    url.searchParams = new URLSearchParams(
        "from=2026-09-01&to=2026-09-08&q=采购",
    )
})

function setup() {
    const patchFilterUrl = vi.fn()
    const searchInputRef = { current: null }
    const hook = renderHook(() =>
        useAccessListFilters({
            view: "audit",
            patchFilterUrl,
            searchInputRef,
        }),
    )
    return { ...hook, patchFilterUrl }
}

test("时间属于常用筛选，单独带时间进入不展开更多条件", () => {
    const { result } = setup()
    expect(result.current.panelOpen).toBe(false)
    expect(result.current.hasStructuredFilters).toBe(true)
})

test("重置更多条件保留日期、关键词及尚未提交的日期错误", () => {
    const { result, patchFilterUrl } = setup()
    act(() => {
        result.current.updateDraft("actorId", "财务")
        result.current.updateDraft("from", "2026-09-10")
    })
    act(() => result.current.applyFilters())
    expect(result.current.filterError).toBeTruthy()
    act(() => result.current.resetMoreFilters())
    expect(result.current.draft).toMatchObject({
        from: "2026-09-10",
        to: "2026-09-08",
        q: "采购",
        actorId: "",
    })
    expect(result.current.filterError).toBeTruthy()
    expect(patchFilterUrl).not.toHaveBeenCalled()
})

test("常用时间修改仍在查询时写入 URL，清除全部同时移除日期", () => {
    const { result, patchFilterUrl } = setup()
    act(() => result.current.updateDraft("from", "2026-09-03"))
    expect(patchFilterUrl).not.toHaveBeenCalled()
    act(() => result.current.applyFilters())
    expect(patchFilterUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({
            from: "2026-09-03",
            to: "2026-09-08",
            page: null,
        }),
    )
    act(() => result.current.clearAllFilters())
    expect(result.current.draft.from).toBe("")
    expect(patchFilterUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({ from: null, to: null }),
    )
})
