import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import {
    buildConnectionAppliedChips,
    useConnectionListFilters,
} from "./use-connection-list-filters"

function urlState(
    overrides: Partial<ConnectionsUrlState> = {},
): ConnectionsUrlState {
    return {
        environment: "PRODUCTION",
        page: 1,
        pageSize: 20,
        section: "overview",
        ...overrides,
    }
}

describe("useConnectionListFilters", () => {
    it("applies all drafts in a single patch and closes the panel", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(urlState(), patchUrl),
        )
        act(() => {
            result.current.setSearchDraft("CONN")
            result.current.setStatusDraft("FAULTED")
            result.current.setHealthDraft(["FAILED", "UNKNOWN"])
            result.current.setCapabilityDraft("PRICE")
            result.current.setCatalogFreshnessDraft(["STALE"])
            result.current.setSupplierIdDraft("s1")
        })
        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            q: "CONN",
            status: "FAULTED",
            health: "FAILED,UNKNOWN",
            capability: "PRICE",
            catalogFreshness: "STALE",
            supplierId: "s1",
            page: 1,
        })
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("omits default values from the apply patch", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(urlState(), patchUrl),
        )
        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            status: undefined,
            health: undefined,
            capability: undefined,
            catalogFreshness: undefined,
            supplierId: undefined,
            page: 1,
        })
    })

    it("clears every filter but keeps the view and navigation params", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(
                urlState({
                    q: "x",
                    status: "ENABLED",
                    health: "FAILED",
                    capability: "ORDER",
                    catalogFreshness: "STALE",
                    supplierId: "s1",
                }),
                patchUrl,
            ),
        )
        act(() => result.current.clearFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            status: undefined,
            health: undefined,
            capability: undefined,
            catalogFreshness: undefined,
            supplierId: undefined,
            page: 1,
        })
        // environment / pageSize 属视图与分页偏好，不被清除
        expect(patchUrl.mock.calls[0][0]).not.toHaveProperty("environment")
        expect(patchUrl.mock.calls[0][0]).not.toHaveProperty("pageSize")
        expect(result.current.searchDraft).toBe("")
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.healthDraft).toEqual([])
        expect(result.current.supplierIdDraft).toBeNull()
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("resets only the structured filters and keeps the panel open", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(
                urlState({ q: "CONN", status: "ENABLED", capability: "ORDER" }),
                patchUrl,
            ),
        )
        act(() => result.current.resetMoreFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            status: undefined,
            health: undefined,
            capability: undefined,
            catalogFreshness: undefined,
            supplierId: undefined,
            page: 1,
        })
        // 关键词与环境保留；面板保持展开
        expect(patchUrl.mock.calls[0][0]).not.toHaveProperty("q")
        expect(patchUrl.mock.calls[0][0]).not.toHaveProperty("environment")
        expect(result.current.searchDraft).toBe("CONN")
        expect(result.current.filterPanelOpen).toBe(true)
    })

    it("removes a single applied condition via its chip key", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(
                urlState({
                    q: "x",
                    status: "ENABLED",
                    health: "FAILED",
                    capability: "ORDER",
                    catalogFreshness: "STALE",
                    supplierId: "s1",
                }),
                patchUrl,
            ),
        )
        act(() => result.current.removeFilter("status"))
        expect(patchUrl).toHaveBeenLastCalledWith({ status: undefined, page: 1 })
        expect(result.current.statusDraft).toBe("all")

        act(() => result.current.removeFilter("health"))
        expect(patchUrl).toHaveBeenLastCalledWith({ health: undefined, page: 1 })
        expect(result.current.healthDraft).toEqual([])

        act(() => result.current.removeFilter("supplierId"))
        expect(patchUrl).toHaveBeenLastCalledWith({
            supplierId: undefined,
            page: 1,
        })
        expect(result.current.supplierIdDraft).toBeNull()

        act(() => result.current.removeFilter("q"))
        expect(patchUrl).toHaveBeenLastCalledWith({ q: undefined, page: 1 })
        expect(result.current.searchDraft).toBe("")
    })

    it("backfills drafts from URL changes without reopening the panel", () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ state }: { state: ConnectionsUrlState }) =>
                useConnectionListFilters(state, patchUrl),
            { initialProps: { state: urlState() } },
        )
        expect(result.current.filterPanelOpen).toBe(false)

        act(() => {
            rerender({
                state: urlState({
                    q: "CONN",
                    status: "FAULTED",
                    catalogFreshness: "STALE,FAILED",
                }),
            })
        })
        expect(result.current.searchDraft).toBe("CONN")
        expect(result.current.statusDraft).toBe("FAULTED")
        expect(result.current.catalogFreshnessDraft).toEqual([
            "STALE",
            "FAILED",
        ])
        // 已挂载页面的 URL 回填不得抢夺当前展开态（§5.5）
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("opens the panel initially when a deep link carries structured filters", () => {
        const { result } = renderHook(() =>
            useConnectionListFilters(
                urlState({ status: "FAULTED", supplierId: "s1" }),
                vi.fn(),
            ),
        )
        expect(result.current.filterPanelOpen).toBe(true)
    })

    it("degrades invalid enum values to defaults", () => {
        const { result } = renderHook(() =>
            useConnectionListFilters(
                urlState({
                    status: "BOGUS",
                    health: "FAILED,NOPE",
                    capability: "WRONG",
                    catalogFreshness: "STALE,X",
                }),
                vi.fn(),
            ),
        )
        expect(result.current.applied.status).toBeUndefined()
        expect(result.current.applied.health).toEqual(["FAILED"])
        expect(result.current.applied.capability).toBeUndefined()
        expect(result.current.applied.catalogFreshness).toEqual(["STALE"])
        expect(result.current.hasStructuredFilters).toBe(true)
        expect(result.current.hasFilters).toBe(true)
    })

    it("applies the environment directly as a view-class parameter", () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useConnectionListFilters(urlState(), patchUrl),
        )
        act(() => result.current.applyEnvironment("STAGING"))
        expect(patchUrl).toHaveBeenCalledWith({ environment: "STAGING", page: 1 })
    })
})

describe("buildConnectionAppliedChips", () => {
    it("builds removable chips for every applied filter with business labels", () => {
        const chips = buildConnectionAppliedChips(
            urlState({
                q: "CONN",
                status: "ENABLED",
                health: "FAILED,UNKNOWN",
                capability: "PRICE",
                catalogFreshness: "STALE",
                supplierId: "s1",
            }),
            "供应商甲",
        )
        expect(chips).toEqual([
            { key: "q", label: "搜索：CONN" },
            { key: "status", label: "状态：启用" },
            { key: "health", label: "健康：失败、结果未知" },
            { key: "capability", label: "能力：价格" },
            { key: "catalogFreshness", label: "目录更新时间：目录陈旧" },
            { key: "supplierId", label: "供应商：供应商甲" },
        ])
    })

    it("falls back to 已选择 when the supplier name is unavailable", () => {
        const chips = buildConnectionAppliedChips(
            urlState({ supplierId: "s1" }),
        )
        expect(chips).toEqual([{ key: "supplierId", label: "供应商：已选择" }])
    })

    it("ignores invalid enum values when building chips", () => {
        const chips = buildConnectionAppliedChips(
            urlState({ status: "BOGUS", q: "  " }),
        )
        expect(chips).toEqual([])
    })
})
