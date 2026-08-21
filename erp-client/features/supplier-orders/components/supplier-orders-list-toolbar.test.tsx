import * as React from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

vi.mock("@/features/entity-selectors", () => ({
    SupplierSearchCombobox: () => null,
}))

import {
    SupplierOrdersListToolbar,
    type SupplierOrdersListToolbarProps,
} from "./supplier-orders-list-toolbar"

function baseProps(
    overrides: Partial<SupplierOrdersListToolbarProps> = {},
): SupplierOrdersListToolbarProps {
    return {
        searchInputRef: React.createRef<HTMLInputElement | null>(),
        view: "actionable",
        onViewChange: vi.fn(),
        searchDraft: "",
        onSearchDraftChange: vi.fn(),
        panelOpen: false,
        setPanelOpen: vi.fn(),
        hasStructuredFilters: false,
        appliedChips: [],
        onRemoveFilter: vi.fn(),
        onApplyFilters: vi.fn(),
        onClearAllFilters: vi.fn(),
        onResetMoreFilters: vi.fn(),
        filterError: null,
        setFilterError: vi.fn(),
        supplierIdDraft: null,
        setSupplierIdDraft: vi.fn(),
        fulfillmentStatusesDraft: [],
        setFulfillmentStatusesDraft: vi.fn(),
        cancelStatusesDraft: [],
        setCancelStatusesDraft: vi.fn(),
        refundStatusesDraft: [],
        setRefundStatusesDraft: vi.fn(),
        paidFromDraft: "",
        setPaidFromDraft: vi.fn(),
        paidToDraft: "",
        setPaidToDraft: vi.fn(),
        ...overrides,
    }
}

afterEach(() => {
    cleanup()
})

describe("SupplierOrdersListToolbar — single form and submit paths", () => {
    it("renders one form with no inline submit button and no panel button", () => {
        const { container } = render(
            <SupplierOrdersListToolbar {...baseProps()} />,
        )

        expect(container.querySelectorAll("form").length).toBe(1)
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: /更多筛选/ }).getAttribute(
                "aria-expanded",
            ),
        ).toBe("false")
    })

    it("submits through the form-level submit and the panel button with the same apply function", () => {
        const onApplyFilters = vi.fn()
        const props = baseProps({ onApplyFilters })
        const { container, rerender } = render(
            <SupplierOrdersListToolbar {...props} />,
        )

        fireEvent.submit(container.querySelector("form")!)
        expect(onApplyFilters).toHaveBeenCalledTimes(1)

        rerender(<SupplierOrdersListToolbar {...props} panelOpen />)
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        fireEvent.click(
            screen.getByRole("button", { name: "应用全部筛选" }),
        )
        expect(onApplyFilters).toHaveBeenCalledTimes(2)
    })

    it("submits the same apply on a form-level submit (Enter path)", () => {
        const onApplyFilters = vi.fn()
        const { container } = render(
            <SupplierOrdersListToolbar
                {...baseProps({ onApplyFilters })}
            />,
        )

        fireEvent.submit(container.querySelector("form")!)
        expect(onApplyFilters).toHaveBeenCalledTimes(1)
    })

    it("toggles the panel via the more-filters button and keeps the panel inside the same form", () => {
        const setPanelOpen = vi.fn()
        const { container } = render(
            <SupplierOrdersListToolbar
                {...baseProps({ setPanelOpen })}
            />,
        )

        fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
        expect(setPanelOpen).toHaveBeenCalledTimes(1)
        expect(typeof setPanelOpen.mock.calls[0]![0]).toBe("function")

        const { container: expandedContainer } = render(
            <SupplierOrdersListToolbar {...baseProps({ panelOpen: true })} />,
        )
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).not.toBeNull()
        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).not.toBeNull()
        expect(container.querySelectorAll("form").length).toBe(1)
        expect(
            expandedContainer.querySelectorAll(
                'button[aria-expanded="true"]',
            ).length,
        ).toBe(1)
    })
})

describe("SupplierOrdersListToolbar — chips and states", () => {
    it("shows applied chips with 已筛选 and 清空全部", () => {
        const onRemoveFilter = vi.fn()
        const onClearAllFilters = vi.fn()
        render(
            <SupplierOrdersListToolbar
                {...baseProps({
                    appliedChips: [
                        { key: "q", label: "搜索：SFO-1" },
                        { key: "supplierId", label: "供应商：华东商贸" },
                    ],
                    onRemoveFilter,
                    onClearAllFilters,
                })}
            />,
        )

        expect(screen.getByText("已筛选")).not.toBeNull()
        expect(screen.getByText("搜索：SFO-1")).not.toBeNull()
        expect(screen.getByText("供应商：华东商贸")).not.toBeNull()

        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(onClearAllFilters).toHaveBeenCalledTimes(1)

        fireEvent.click(
            screen.getByRole("button", { name: "移除搜索：SFO-1" }),
        )
        expect(onRemoveFilter).toHaveBeenCalledWith("q")
    })

    it("shows the 已启用 badge only for applied structured filters", () => {
        const { rerender } = render(
            <SupplierOrdersListToolbar {...baseProps()} />,
        )
        expect(screen.queryByText("已启用")).toBeNull()

        rerender(
            <SupplierOrdersListToolbar
                {...baseProps({ hasStructuredFilters: true })}
            />,
        )
        expect(screen.getByText("已启用")).not.toBeNull()
    })

    it("renders the paid range error with role=alert", () => {
        render(
            <SupplierOrdersListToolbar
                {...baseProps({
                    panelOpen: true,
                    filterError: "支付开始日期不能晚于结束日期",
                })}
            />,
        )

        expect(screen.getByRole("alert").textContent).toBe(
            "支付开始日期不能晚于结束日期",
        )
    })
})
