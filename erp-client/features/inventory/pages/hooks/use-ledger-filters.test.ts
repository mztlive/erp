import { describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import {
    useLedgerFilters,
    type UseLedgerFiltersInput,
} from "./use-ledger-filters"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

function makeProps(
    overrides: Partial<UseLedgerFiltersInput> = {},
): UseLedgerFiltersInput {
    return {
        view: "balance",
        warehouseId: undefined,
        availability: "all",
        movementType: [],
        occurredFrom: undefined,
        occurredTo: undefined,
        searchDraft: "",
        setSearchDraft: vi.fn(),
        patchUrl: vi.fn<LedgerPatchUrl>(),
        resetPagination: vi.fn(),
        ...overrides,
    }
}

function setup(overrides: Partial<UseLedgerFiltersInput> = {}) {
    const props = makeProps(overrides)
    const rendered = renderHook(
        (input: UseLedgerFiltersInput) => useLedgerFilters(input),
        { initialProps: props },
    )
    return {
        ...rendered,
        patchUrl: props.patchUrl,
        resetPagination: props.resetPagination,
        props,
    }
}

describe("useLedgerFilters", () => {
    it("initialises drafts from the applied URL values", () => {
        const { result } = setup({
            view: "movement",
            warehouseId: "w1",
            availability: "zero",
            movementType: ["A", "B"],
            occurredFrom: "2026-08-01",
            occurredTo: "2026-08-14",
            searchDraft: "SKU-1",
        })
        expect(result.current.warehouseIdDraft).toBe("w1")
        expect(result.current.availabilityDraft).toBe("zero")
        expect(result.current.movementTypeDraft).toEqual(["A", "B"])
        expect(result.current.occurredFromDraft).toBe("2026-08-01")
        expect(result.current.occurredToDraft).toBe("2026-08-14")
    })

    it("opens the panel on initial deep links with structured filters", () => {
        const none = setup({ view: "balance" })
        expect(none.result.current.panelOpen).toBe(false)
        none.unmount()

        const warehouse = setup({ view: "balance", warehouseId: "w1" })
        expect(warehouse.result.current.panelOpen).toBe(true)
        warehouse.unmount()

        const movement = setup({
            view: "movement",
            movementType: ["A"],
        })
        expect(movement.result.current.panelOpen).toBe(true)
        movement.unmount()
    })

    it("draft changes never touch the URL", () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setWarehouseIdDraft("w9")
            result.current.setAvailabilityDraft("zero")
            result.current.setMovementTypeDraft(["A"])
            result.current.setOccurredFromDraft("2026-08-01")
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("applyFilters patches every filter once, resets pagination and closes the panel", () => {
        const { result, patchUrl, resetPagination } = setup({
            view: "movement",
            searchDraft: "  SKU-1  ",
        })
        act(() => {
            result.current.setWarehouseIdDraft("w1")
            result.current.setAvailabilityDraft("zero")
            result.current.setMovementTypeDraft(["B", "A", "B"])
            result.current.setOccurredFromDraft("2026-08-01")
            result.current.setOccurredToDraft("2026-08-14")
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith(
            {
                q: "SKU-1",
                warehouseId: "w1",
                availability: "zero",
                movementType: "A,B",
                occurredFrom: "2026-08-01",
                occurredTo: "2026-08-14",
            },
            { replace: true, scroll: false },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
        expect(result.current.panelOpen).toBe(false)
    })

    it("applyFilters drops defaults from the URL instead of writing them", () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setSearchDraft("   ")
            result.current.setAvailabilityDraft("all")
            result.current.setMovementTypeDraft([])
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(patchUrl).toHaveBeenCalledWith(
            {
                q: null,
                warehouseId: null,
                availability: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
            },
            { replace: true, scroll: false },
        )
    })

    it("keeps the panel open and writes nothing when the date range is inverted", () => {
        const { result, patchUrl } = setup({ view: "movement" })
        act(() => {
            result.current.setOccurredFromDraft("2026-08-10")
            result.current.setOccurredToDraft("2026-08-01")
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(result.current.filterError).toBe("截止日期不能早于起始日期")
        expect(patchUrl).not.toHaveBeenCalled()
        expect(result.current.panelOpen).toBe(true)
    })

    it("removeFilter removes a single applied condition and resets pagination", () => {
        const { result, patchUrl, resetPagination } = setup({
            warehouseId: "w1",
        })
        act(() => {
            result.current.removeFilter("warehouseId")
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { warehouseId: null },
            { replace: true, scroll: false },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)

        act(() => {
            result.current.removeFilter("occurredRange")
        })
        expect(patchUrl).toHaveBeenLastCalledWith(
            { occurredFrom: null, occurredTo: null },
            { replace: true, scroll: false },
        )
    })

    it("resetMoreFilters clears only structured conditions, keeps q and source locks, and keeps the panel open", () => {
        const { result, patchUrl } = setup({
            view: "movement",
            searchDraft: "SKU-1",
        })
        act(() => {
            result.current.setWarehouseIdDraft("w1")
            result.current.setMovementTypeDraft(["A"])
            result.current.setOccurredFromDraft("2026-08-01")
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.resetMoreFilters()
        })
        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith(
            {
                warehouseId: null,
                availability: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
            },
            { replace: true, scroll: false },
        )
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.warehouseIdDraft).toBeNull()
        expect(result.current.movementTypeDraft).toEqual([])
        expect(result.current.occurredFromDraft).toBe("")
    })

    it("clearAllFilters resets drafts, error, panel and every filter param including source locks", () => {
        const { result, patchUrl, resetPagination } = setup({
            warehouseId: "w1",
            searchDraft: "SKU-1",
        })
        act(() => {
            result.current.setOccurredFromDraft("2026-08-01")
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.clearAllFilters()
        })
        expect(result.current.setSearchDraft).toHaveBeenCalledWith("")
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.warehouseIdDraft).toBeNull()
        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith(
            {
                q: null,
                warehouseId: null,
                availability: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
            },
            { replace: true, scroll: false },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it("backfills drafts from URL changes without stealing the panel state", () => {
        const { result, rerender } = setup({
            view: "balance",
            warehouseId: "w1",
            availability: "zero",
        })
        expect(result.current.panelOpen).toBe(true)
        act(() => {
            result.current.setPanelOpen(false)
        })

        rerender(
            makeProps({
                view: "balance",
                warehouseId: "w2",
                availability: "zero",
            }),
        )
        expect(result.current.warehouseIdDraft).toBe("w2")
        expect(result.current.availabilityDraft).toBe("zero")
        // 面板展开态不被 URL 回填重置
        expect(result.current.panelOpen).toBe(false)
    })

    it("hasStructuredFilters only counts conditions consumed on the active view", () => {
        const availability = setup({
            view: "balance",
            availability: "zero",
        })
        expect(availability.result.current.hasStructuredFilters).toBe(true)
        availability.unmount()

        const inert = setup({ view: "balance", movementType: ["A"] })
        expect(inert.result.current.hasStructuredFilters).toBe(false)
        inert.unmount()

        const movement = setup({
            view: "movement",
            occurredFrom: "2026-08-01",
        })
        expect(movement.result.current.hasStructuredFilters).toBe(true)
        movement.unmount()
    })
})
