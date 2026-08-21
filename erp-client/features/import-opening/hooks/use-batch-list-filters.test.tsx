import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { useBatchListFilters } from "@/features/import-opening/hooks/use-batch-list-filters"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"

function state(
    overrides?: Partial<ImportOpeningUrlState>,
): ImportOpeningUrlState {
    return {
        environment: "VALIDATION",
        section: "overview",
        page: 1,
        ...overrides,
    }
}

function setup(overrides?: Partial<ImportOpeningUrlState>) {
    const patchUrl = vi.fn()
    const view = renderHook(
        ({ urlState }: { urlState: ImportOpeningUrlState }) =>
            useBatchListFilters({ urlState, patchUrl }),
        { initialProps: { urlState: state(overrides) } },
    )
    return { patchUrl, ...view }
}

describe("useBatchListFilters", () => {
    it("initializes drafts from the applied URL state", () => {
        const { result } = setup({
            q: "B-01",
            status: "APPLYING",
            objectType: "SKU",
        })
        expect(result.current.qDraft).toBe("B-01")
        expect(result.current.statusDraft).toBe("APPLYING")
        expect(result.current.objectTypeDraft).toBe("SKU")
    })

    it("opens the panel initially when a deep link carries structured filters", () => {
        const { result } = setup({ status: "RECEIVING" })
        expect(result.current.batchFilterPanelOpen).toBe(true)
    })

    it("keeps the panel closed initially without structured filters", () => {
        const { result } = setup({ q: "B-01" })
        expect(result.current.batchFilterPanelOpen).toBe(false)
    })

    it("degrades illegal enum values in the URL to the default", () => {
        const { result } = setup({ status: "not-a-status" })
        expect(result.current.appliedStatus).toBeUndefined()
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.hasAppliedBatchFilters).toBe(false)
    })

    it("never writes the URL while drafts change", () => {
        const { patchUrl, result } = setup()
        act(() => result.current.setQDraft("abc"))
        act(() => result.current.setStatusDraft("FAILED"))
        act(() => result.current.setObjectTypeDraft("CUSTOMER"))
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("applies all drafts in one URL write, closes the panel and resets the page", () => {
        const { patchUrl, result } = setup({ page: 3, q: "x" })
        act(() => result.current.setBatchFilterPanelOpen(true))
        act(() => {
            result.current.setQDraft("  B-01 ")
            result.current.setStatusDraft("AWAITING_CONFIRMATION")
            result.current.setObjectTypeDraft("CUSTOMER")
        })
        act(() => result.current.applyBatchFilters())
        expect(patchUrl).toHaveBeenCalledWith({
            q: "B-01",
            status: "AWAITING_CONFIRMATION",
            objectType: "CUSTOMER",
            page: 1,
        })
        expect(result.current.batchFilterPanelOpen).toBe(false)
    })

    it("drops all-default drafts from the URL on apply", () => {
        const { patchUrl, result } = setup()
        act(() => result.current.applyBatchFilters())
        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            objectType: undefined,
            status: undefined,
            page: 1,
        })
    })

    it("removes a single applied condition with its draft and resets the page", () => {
        const { patchUrl, result } = setup({ status: "FAILED", q: "B-01" })
        act(() => result.current.removeBatchFilter("status"))
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.qDraft).toBe("B-01")
        expect(patchUrl).toHaveBeenCalledWith({ status: undefined, page: 1 })
    })

    it("clears all filters: drafts, panel, URL params and page", () => {
        const { patchUrl, result } = setup({
            q: "B-01",
            status: "FAILED",
            objectType: "SKU",
            page: 4,
        })
        act(() => result.current.setBatchFilterPanelOpen(true))
        act(() => result.current.clearAllBatchFilters())
        expect(result.current.qDraft).toBe("")
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.objectTypeDraft).toBe("all")
        expect(result.current.batchFilterPanelOpen).toBe(false)
        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            objectType: undefined,
            status: undefined,
            page: 1,
        })
    })

    it("reset more filters keeps q and the panel open", () => {
        const { patchUrl, result } = setup({
            q: "B-01",
            status: "FAILED",
            objectType: "SKU",
        })
        act(() => result.current.setBatchFilterPanelOpen(true))
        act(() => result.current.resetMoreBatchFilters())
        expect(result.current.qDraft).toBe("B-01")
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.objectTypeDraft).toBe("all")
        expect(result.current.batchFilterPanelOpen).toBe(true)
        expect(patchUrl).toHaveBeenCalledWith({
            objectType: undefined,
            status: undefined,
            page: 1,
        })
    })

    it("syncs structured drafts back from the URL without forcing the panel open", () => {
        const { patchUrl, result, rerender } = setup({ q: "B-01" })
        act(() => result.current.setBatchFilterPanelOpen(false))
        rerender({ urlState: state({ q: "B-02", status: "FAILED" }) })
        expect(result.current.qDraft).toBe("B-02")
        expect(result.current.statusDraft).toBe("FAILED")
        expect(result.current.batchFilterPanelOpen).toBe(false)
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("builds chips only from applied conditions", () => {
        const { result } = setup({ q: "B-01", status: "FAILED" })
        expect(result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：B-01" },
            { key: "status", label: "状态：失败" },
        ])
    })

    it("builds an object chip with the business label", () => {
        const { result } = setup({ objectType: "CARD_SALES_ORDER" })
        expect(result.current.appliedChips).toEqual([
            { key: "objectType", label: "对象：卡券销售单" },
        ])
    })
})
