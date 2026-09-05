import { createRef } from "react"
import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import { useSupplierOrdersFilters } from "@/features/supplier-orders/hooks/use-supplier-orders-filters"
import type { SupplierOrdersUrlState } from "@/features/supplier-orders/lib/url-state"

const state = vi.hoisted(() => ({
    url: { view: "all", page: 3, pageSize: 20 } as SupplierOrdersUrlState,
    updateUrl: vi.fn(),
}))

vi.mock(
    "@/features/supplier-orders/hooks/use-supplier-orders-url-state",
    () => ({
        useSupplierOrdersUrlState: () => ({ ...state, returnTo: undefined }),
    }),
)
vi.mock("@/features/entity-selectors/hooks/queries", () => ({
    useSupplierSelectorQuery: () => ({ selected: { data: undefined } }),
}))

beforeEach(() => {
    state.url = { view: "all", page: 3, pageSize: 20 } as SupplierOrdersUrlState
    state.updateUrl.mockClear()
})
afterEach(cleanup)

function setup() {
    const inputRef = createRef<HTMLInputElement>()
    return renderHook(() => useSupplierOrdersFilters(inputRef))
}

test("售后条件先修改草稿，查询后生效并回到第一页", () => {
    const { result } = setup()
    act(() => result.current.setAftersalePendingDraft(true))
    expect(state.updateUrl).not.toHaveBeenCalled()
    expect(result.current.hasPendingChanges).toBe(true)
    act(() => result.current.applyFilters())
    expect(state.updateUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({ aftersalePending: true, page: 1 }),
    )
})

test("已生效售后条件支持 URL 回填、移除和清除全部", () => {
    const { result, rerender } = setup()
    state.url = { ...state.url, aftersalePending: true }
    rerender()
    expect(result.current.aftersalePendingDraft).toBe(true)
    expect(result.current.appliedChips).toContainEqual({
        key: "aftersalePending",
        label: "售后待处理",
    })
    act(() => result.current.removeFilter("aftersalePending"))
    expect(result.current.aftersalePendingDraft).toBe(false)
    expect(state.updateUrl).toHaveBeenLastCalledWith({
        aftersalePending: undefined,
        page: 1,
    })
    act(() => result.current.setAftersalePendingDraft(true))
    act(() => result.current.clearAllFilters())
    expect(result.current.aftersalePendingDraft).toBe(false)
    expect(state.updateUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({ aftersalePending: undefined, page: 1 }),
    )
})

test("重置更多条件保留当前查询，提交后才清除 URL", () => {
    state.url = { ...state.url, aftersalePending: true }
    const { result } = setup()
    expect(result.current.panelOpen).toBe(true)
    act(() => result.current.resetMoreFilters())
    expect(result.current.aftersalePendingDraft).toBe(false)
    expect(result.current.hasPendingChanges).toBe(true)
    expect(state.updateUrl).not.toHaveBeenCalled()
    act(() => result.current.applyFilters())
    expect(state.updateUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({ aftersalePending: undefined, page: 1 }),
    )
})
