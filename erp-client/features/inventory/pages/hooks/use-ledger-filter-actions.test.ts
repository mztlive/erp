import { describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useLedgerFilterActions } from "./use-ledger-filter-actions"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

function setup(sortValue = "") {
    const patchUrl = vi.fn<LedgerPatchUrl>()
    const resetPagination = vi.fn()
    const rendered = renderHook(() =>
        useLedgerFilterActions({
            patchUrl,
            resetPagination,
            sortValue,
        }),
    )
    return {
        result: rendered.result,
        patchUrl,
        resetPagination,
    }
}

describe("useLedgerFilterActions", () => {
    it("view change writes the view and drops a sort that does not belong to it", () => {
        const { result, patchUrl, resetPagination } = setup(
            "occurredAt:desc,movementId:desc",
        )
        act(() => {
            result.current.handleViewChange("balance")
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: "balance", sort: null },
            { replace: true },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it("view change keeps a sort value that is valid for the target view", () => {
        const { result, patchUrl } = setup("occurredAt:desc,movementId:desc")
        act(() => {
            result.current.handleViewChange("movement")
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: "movement" },
            { replace: true },
        )
    })

    it("view change keeps no sort when none is set", () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.handleViewChange("adjustment")
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: "adjustment" },
            { replace: true },
        )
    })

    it("sort change writes the sort with replace and resets pagination", () => {
        const { result, patchUrl, resetPagination } = setup()
        act(() => {
            result.current.handleSortChange("lastMovementAt:desc,skuCode:asc")
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { sort: "lastMovementAt:desc,skuCode:asc" },
            { replace: true },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it("sort change with an empty value removes the sort param", () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.handleSortChange("")
        })
        expect(patchUrl).toHaveBeenCalledWith({ sort: null }, { replace: true })
    })
})
