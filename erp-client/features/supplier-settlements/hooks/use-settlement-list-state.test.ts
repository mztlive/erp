import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import { useSettlementListState } from "./use-settlement-list-state"

function makeState(
    overrides: Partial<SettlementsUrlState> = {},
): SettlementsUrlState {
    return {
        view: "pending",
        page: 1,
        section: "overview",
        ...overrides,
    }
}

function renderFilters(
    state: SettlementsUrlState = makeState(),
    patchUrl: (patch: Partial<SettlementsUrlState>) => void = vi.fn(),
) {
    const searchInputRef = { current: null as HTMLInputElement | null }
    return {
        ...renderHook(() => useSettlementListState(state, patchUrl, searchInputRef)),
        patchUrl,
    }
}

describe("useSettlementListState", () => {
    it("derives the first page from a default url state", () => {
        const { result } = renderFilters()

        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 50,
        })
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.panelOpen).toBe(false)
    })

    it("maps the url page to a zero-based page index and clamps at zero", () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ page }: { page: number }) =>
                useSettlementListState(makeState({ page }), patchUrl, {
                    current: null,
                }),
            { initialProps: { page: 3 } },
        )

        expect(result.current.pagination.pageIndex).toBe(2)

        rerender({ page: 0 })
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("detects active filters from any filter field; view alone is not a filter", () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ state }: { state: SettlementsUrlState }) =>
                useSettlementListState(state, patchUrl, { current: null }),
            { initialProps: { state: makeState() } },
        )

        for (const patch of [
            { supplierId: "sup1" },
            { periodFrom: "2026-01-01" },
            { periodTo: "2026-01-31" },
            { status: "DRAFT,PENDING_REVIEW" },
            { differenceType: "AMOUNT" as const },
            { q: "abc" },
        ]) {
            rerender({ state: makeState(patch) })
            expect(result.current.hasActiveFilters).toBe(true)
        }

        rerender({ state: makeState({ view: "confirmed" }) })
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it("opens the more-filter panel for deep links with structured conditions", () => {
        const { result } = renderFilters(
            makeState({ status: "DRAFT", periodFrom: "2026-01-01" }),
        )

        expect(result.current.panelOpen).toBe(true)
        expect(result.current.statusDraft).toEqual(["DRAFT"])
    })

    it("drops illegal enum values when parsing status from the url", () => {
        const { result } = renderFilters(
            makeState({ status: "DRAFT,UNKNOWN,PENDING_REVIEW" }),
        )

        expect(result.current.statusDraft).toEqual([
            "DRAFT",
            "PENDING_REVIEW",
        ])
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it("applies all drafts in one patch, returns to page 1 and closes the panel", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(makeState({ page: 3 }), patchUrl)

        act(() => result.current.setSearchDraft("  st-1  "))
        act(() => result.current.setSupplierIdDraft("sup1"))
        act(() => result.current.setStatusDraft(["DRAFT", "VOIDED"]))
        act(() => result.current.setDifferenceTypeDraft("AMOUNT"))
        act(() => result.current.setPeriodFromDraft("2026-01-01"))
        act(() => result.current.setPeriodToDraft("2026-01-31"))
        act(() => result.current.setPanelOpen(true))
        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            q: "st-1",
            supplierId: "sup1",
            status: "DRAFT,VOIDED",
            differenceType: "AMOUNT",
            periodFrom: "2026-01-01",
            periodTo: "2026-01-31",
            page: 1,
        })
        expect(result.current.panelOpen).toBe(false)
    })

    it("omits default values when applying filters", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(makeState(), patchUrl)

        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
    })

    it("rejects an invalid period range without patching the url", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(makeState(), patchUrl)

        act(() => result.current.setPeriodFromDraft("2026-02-01"))
        act(() => result.current.setPeriodToDraft("2026-01-31"))
        act(() => result.current.applyFilters())

        expect(result.current.periodError).toBe(
            "期间开始日期不能晚于结束日期",
        )
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("clears an edited period error on the next apply", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(makeState(), patchUrl)

        act(() => result.current.setPeriodFromDraft("2026-02-01"))
        act(() => result.current.setPeriodToDraft("2026-01-31"))
        act(() => result.current.applyFilters())
        act(() => result.current.setPeriodFromDraft("2026-01-15"))
        act(() => result.current.applyFilters())

        expect(result.current.periodError).toBeNull()
        expect(patchUrl).toHaveBeenCalledTimes(1)
    })

    it("clears filters but preserves view, period and navigation context", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(
            makeState({
                view: "confirmed",
                supplierId: "sup1",
                status: "DRAFT",
                differenceType: "AMOUNT",
                q: "abc",
                periodFrom: "2026-01-01",
                periodTo: "2026-01-31",
                page: 3,
                preview: "st1",
                returnTo: "/workspace/tasks",
            }),
            patchUrl,
        )

        act(() => result.current.setPanelOpen(true))
        act(() => result.current.clearAllFilters())

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            page: 1,
        })
        expect(result.current.searchDraft).toBe("")
        expect(result.current.supplierIdDraft).toBeNull()
        expect(result.current.statusDraft).toEqual([])
        expect(result.current.differenceTypeDraft).toBe("all")
        expect(result.current.panelOpen).toBe(false)
    })

    it("resets only more-filter conditions, keeps search and keeps the panel open", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(
            makeState({
                q: "abc",
                supplierId: "sup1",
                status: "DRAFT",
                differenceType: "AMOUNT",
                periodFrom: "2026-01-01",
                periodTo: "2026-01-31",
            }),
            patchUrl,
        )

        act(() => result.current.setPanelOpen(true))
        act(() => result.current.resetMoreFilters())

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
        expect(result.current.supplierIdDraft).toBeNull()
        expect(result.current.statusDraft).toEqual([])
        expect(result.current.differenceTypeDraft).toBe("all")
        expect(result.current.periodFromDraft).toBe("")
        expect(result.current.panelOpen).toBe(true)
    })

    it("removes a single applied condition without touching the others", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(
            makeState({ q: "abc", status: "DRAFT", periodFrom: "2026-01-01" }),
            patchUrl,
        )

        act(() => result.current.removeFilter("status"))

        expect(patchUrl).toHaveBeenCalledWith({
            status: undefined,
            page: 1,
        })
        expect(result.current.statusDraft).toEqual([])
        expect(result.current.searchDraft).toBe("abc")
    })

    it("removes both period bounds as one condition", () => {
        const patchUrl = vi.fn()
        const { result } = renderFilters(
            makeState({ periodFrom: "2026-01-01", periodTo: "2026-01-31" }),
            patchUrl,
        )

        act(() => result.current.removeFilter("period"))

        expect(patchUrl).toHaveBeenCalledWith({
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
        expect(result.current.periodFromDraft).toBe("")
        expect(result.current.periodToDraft).toBe("")
    })

    it("backfills drafts when the url changes without stealing panel state", () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ state }: { state: SettlementsUrlState }) =>
                useSettlementListState(state, patchUrl, { current: null }),
            { initialProps: { state: makeState() } },
        )

        act(() => result.current.setPanelOpen(true))
        act(() => result.current.setSearchDraft("typed-but-unsubmitted"))

        rerender({
            state: makeState({ supplierId: "sup1", status: "DRAFT" }),
        })

        expect(result.current.supplierIdDraft).toBe("sup1")
        expect(result.current.statusDraft).toEqual(["DRAFT"])
        // 回填只同步 Draft，不得抢夺用户当前展开态
        expect(result.current.panelOpen).toBe(true)
    })

    it("keeps a stable clearAllFilters callback while patchUrl is stable", () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({
                state,
                patchUrl: patch,
            }: {
                state: SettlementsUrlState
                patchUrl: (patch: Partial<SettlementsUrlState>) => void
            }) => useSettlementListState(state, patch, { current: null }),
            { initialProps: { state: makeState(), patchUrl } },
        )

        const first = result.current.clearAllFilters
        rerender({ state: makeState({ q: "abc" }), patchUrl })
        expect(result.current.clearAllFilters).toBe(first)

        const nextPatchUrl = vi.fn()
        rerender({ state: makeState(), patchUrl: nextPatchUrl })
        expect(result.current.clearAllFilters).not.toBe(first)
    })
})
