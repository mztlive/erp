import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"
import { useLedgerFilters } from "./use-ledger-filters"
import type { InventoryView } from "@/features/inventory/types"

afterEach(cleanup)

function setup(view: InventoryView) {
    const patchUrl = vi.fn()
    const resetPagination = vi.fn()
    const hook = renderHook(() =>
        useLedgerFilters({
            view,
            q: "",
            warehouseId: "warehouse-a",
            availability: "all",
            movementType: view === "movement" ? ["COUNT_GAIN"] : [],
            occurredFrom: undefined,
            occurredTo: undefined,
            searchDraft: "",
            setSearchDraft: vi.fn(),
            patchUrl,
            resetPagination,
        }),
    )
    return { ...hook, patchUrl, resetPagination }
}

test("余额仓库作为常用条件不展开更多面板，修改条件须查询后生效且可单独清除", () => {
    const { result, patchUrl, resetPagination } = setup("balance")
    expect(result.current.panelOpen).toBe(false)
    act(() => result.current.setAvailabilityDraft("reserved"))
    expect(patchUrl).not.toHaveBeenCalled()
    expect(result.current.hasPendingChanges).toBe(true)
    act(() => result.current.applyFilters())
    expect(patchUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({
            warehouseId: "warehouse-a",
            availability: "reserved",
        }),
        { replace: true, scroll: false },
    )
    expect(resetPagination).toHaveBeenCalledTimes(1)
    act(() => result.current.removeFilter("availability"))
    expect(result.current.availabilityDraft).toBe("all")
    expect(result.current.warehouseIdDraft).toBe("warehouse-a")
    expect(patchUrl).toHaveBeenLastCalledWith(
        { availability: null },
        { replace: true, scroll: false },
    )
})

test("流水重置更多条件仅清流水条件草稿，保留常用仓库与已生效查询", () => {
    const { result, patchUrl } = setup("movement")
    expect(result.current.panelOpen).toBe(true)
    act(() => {
        result.current.setOccurredFromDraft("2026-09-01")
        result.current.setWarehouseIdDraft("warehouse-b")
    })
    act(() => result.current.resetMoreFilters())
    expect(result.current.warehouseIdDraft).toBe("warehouse-b")
    expect(result.current.movementTypeDraft).toEqual([])
    expect(result.current.occurredFromDraft).toBe("")
    expect(patchUrl).not.toHaveBeenCalled()
    act(() => result.current.clearAllFilters())
    expect(result.current.warehouseIdDraft).toBeNull()
    expect(result.current.panelOpen).toBe(false)
    expect(patchUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({
            warehouseId: null,
            availability: null,
            movementType: null,
        }),
        { replace: true, scroll: false },
    )
})
