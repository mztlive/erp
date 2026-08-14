import { fireEvent, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { SupplierOrdersUrlState } from "@/features/supplier-orders/lib/url-state"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"
import { useSupplierOrdersKeyboardNav } from "./use-supplier-orders-keyboard-nav"

function makeUrl(
    overrides: Partial<SupplierOrdersUrlState> = {},
): SupplierOrdersUrlState {
    return {
        view: "actionable",
        page: 1,
        pageSize: 50,
        section: "overview",
        ...overrides,
    }
}

function makeRow(orderId: string): SupplierOrderListRow {
    return {
        orderId,
        orderNo: `SFO-${orderId}`,
        mallOrderId: "mall_1",
        mallOrderNo: "MO-1",
        supplierId: "sup_1",
        supplierName: "华东供应商",
        fulfillmentStatus: "SUBMITTING",
        fulfillmentLabel: "提交中",
        fulfillmentTone: "info",
        cancelStatus: "NONE",
        cancelLabel: "未发起",
        cancelTone: "neutral",
        refundStatus: "NONE",
        refundLabel: "未发起",
        refundTone: "neutral",
        paidAt: "",
        updatedAt: "",
        lastBusinessAt: "",
        itemCount: 1,
        allowedActions: [],
        actionBlockers: [],
        priority: 70,
    }
}

function renderKeyboardNav(
    rows: SupplierOrderListRow[],
    url: SupplierOrdersUrlState,
) {
    const updateUrl = vi.fn()
    const rendered = renderHook(
        ({
            rows,
            url,
        }: {
            rows: SupplierOrderListRow[]
            url: SupplierOrdersUrlState
        }) => useSupplierOrdersKeyboardNav({ rows, url, updateUrl }),
        { initialProps: { rows, url } },
    )
    return { ...rendered, updateUrl }
}

beforeEach(() => {
    document.body.innerHTML = ""
})

describe("useSupplierOrdersKeyboardNav — movement", () => {
    it("moves down with j and up with k, clamped to bounds", () => {
        const rows = [makeRow("a"), makeRow("b")]
        const { result } = renderKeyboardNav(rows, makeUrl())

        fireEvent.keyDown(window, { key: "j" })
        expect(result.current.focusedIndex).toBe(1)
        fireEvent.keyDown(window, { key: "j" })
        expect(result.current.focusedIndex).toBe(1)
        fireEvent.keyDown(window, { key: "k" })
        expect(result.current.focusedIndex).toBe(0)
        fireEvent.keyDown(window, { key: "k" })
        expect(result.current.focusedIndex).toBe(0)
    })

    it("supports arrow keys", () => {
        const rows = [makeRow("a"), makeRow("b"), makeRow("c")]
        const { result } = renderKeyboardNav(rows, makeUrl())

        fireEvent.keyDown(window, { key: "ArrowDown" })
        fireEvent.keyDown(window, { key: "ArrowDown" })
        expect(result.current.focusedIndex).toBe(2)
        fireEvent.keyDown(window, { key: "ArrowUp" })
        expect(result.current.focusedIndex).toBe(1)
    })

    it("ignores movement when the list is empty", () => {
        const { result } = renderKeyboardNav([], makeUrl())

        fireEvent.keyDown(window, { key: "j" })
        expect(result.current.focusedIndex).toBe(0)
    })

    it("ignores keys while typing in an input except Escape", () => {
        const rows = [makeRow("a"), makeRow("b")]
        const { result, updateUrl } = renderKeyboardNav(
            rows,
            makeUrl({ preview: "a" }),
        )
        const input = document.createElement("input")
        document.body.appendChild(input)
        input.focus()

        fireEvent.keyDown(input, { key: "j" })
        expect(result.current.focusedIndex).toBe(0)

        fireEvent.keyDown(input, { key: "Escape" })
        expect(updateUrl).toHaveBeenCalledWith({ preview: undefined }, "push")
    })
})

describe("useSupplierOrdersKeyboardNav — navigation actions", () => {
    it("opens the focused row preview on Enter", () => {
        const rows = [makeRow("a"), makeRow("b")]
        const { result, updateUrl } = renderKeyboardNav(rows, makeUrl())

        fireEvent.keyDown(window, { key: "j" })
        fireEvent.keyDown(window, { key: "Enter" })

        expect(updateUrl).toHaveBeenCalledWith({ preview: "b" }, "push")
        expect(result.current.focusedIndex).toBe(1)
    })

    it("does nothing on Enter with an empty list", () => {
        const { updateUrl } = renderKeyboardNav([], makeUrl())

        fireEvent.keyDown(window, { key: "Enter" })
        expect(updateUrl).not.toHaveBeenCalled()
    })

    it("closes the preview on Escape", () => {
        const rows = [makeRow("a")]
        const { updateUrl } = renderKeyboardNav(rows, makeUrl({ preview: "a" }))

        fireEvent.keyDown(window, { key: "Escape" })
        expect(updateUrl).toHaveBeenCalledWith({ preview: undefined }, "push")
    })

    it("focuses the search input on /", () => {
        const input = document.createElement("input")
        input.setAttribute("data-slot", "sfo-list-search")
        document.body.appendChild(input)

        renderKeyboardNav([makeRow("a")], makeUrl())
        fireEvent.keyDown(window, { key: "/" })

        expect(document.activeElement).toBe(input)
    })
})

describe("useSupplierOrdersKeyboardNav — resets", () => {
    it("resets the focused index when the page or filter changes", () => {
        const rows = [makeRow("a"), makeRow("b")]
        const { result, rerender } = renderKeyboardNav(rows, makeUrl())

        fireEvent.keyDown(window, { key: "j" })
        expect(result.current.focusedIndex).toBe(1)

        rerender({ rows, url: makeUrl({ page: 2 }) })
        expect(result.current.focusedIndex).toBe(0)
    })

    it("resets when the row count changes", () => {
        const rows = [makeRow("a"), makeRow("b")]
        const { result, rerender } = renderKeyboardNav(rows, makeUrl())

        fireEvent.keyDown(window, { key: "j" })
        expect(result.current.focusedIndex).toBe(1)

        rerender({
            rows: [makeRow("a"), makeRow("b"), makeRow("c")],
            url: makeUrl(),
        })
        expect(result.current.focusedIndex).toBe(0)
    })
})
