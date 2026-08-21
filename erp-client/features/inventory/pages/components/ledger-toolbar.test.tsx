import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createFreshQueryClient } from "@/features/test-utils"
import type { LedgerAppliedChip } from "@/features/inventory/pages/hooks/use-ledger-filters"
import type { InventoryView } from "@/features/inventory/types"
import { LedgerToolbar } from "./ledger-toolbar"

afterEach(cleanup)

function renderToolbar({
    view = "balance",
    open = false,
    chips = [],
    hasActiveFilters = false,
    filterError = null,
}: {
    view?: InventoryView
    open?: boolean
    chips?: readonly LedgerAppliedChip[]
    hasActiveFilters?: boolean
    filterError?: string | null
} = {}) {
    const props = {
        view,
        searchInputRef: { current: null },
        searchDraft: "",
        setSearchDraft: vi.fn(),
        warehouseIdDraft: null,
        setWarehouseIdDraft: vi.fn(),
        availabilityDraft: "all" as const,
        setAvailabilityDraft: vi.fn(),
        movementTypeDraft: [] as string[],
        setMovementTypeDraft: vi.fn(),
        occurredFromDraft: "",
        setOccurredFromDraft: vi.fn(),
        occurredToDraft: "",
        setOccurredToDraft: vi.fn(),
        panelOpen: open,
        setPanelOpen: vi.fn(),
        hasStructuredFilters: chips.some((chip) => chip.key !== "q"),
        hasActiveFilters,
        appliedChips: chips,
        removeFilter: vi.fn(),
        applyFilters: vi.fn(),
        resetMoreFilters: vi.fn(),
        clearAllFilters: vi.fn(),
        filterError,
        setFilterError: vi.fn(),
    }
    const utils = render(
        <QueryClientProvider client={createFreshQueryClient()}>
            <LedgerToolbar {...props} />
        </QueryClientProvider>,
    )
    return { ...utils, props }
}

describe("LedgerToolbar", () => {
    it("renders a single semantic form for the whole filter area", () => {
        const { container } = renderToolbar({ open: true })

        expect(container.querySelectorAll("form")).toHaveLength(1)
        expect(
            container.querySelector("form")?.querySelector("input"),
        ).not.toBeNull()
    })

    it("collapsed state has no submit button and no apply-all button while closed", () => {
        renderToolbar()

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        expect(screen.queryByRole("button", { name: "搜索" })).toBeNull()
        expect(screen.queryByRole("radio", { name: "有可用" })).toBeNull()
    })

    it("expanded state shows only the main submit inside the panel", () => {
        const { container } = renderToolbar({ open: true })

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
        expect(screen.getByRole("radio", { name: "全部" })).toBeDefined()
        expect(screen.getByRole("radio", { name: "有可用" })).toBeDefined()
        expect(screen.getByRole("radio", { name: "零可用" })).toBeDefined()
        expect(screen.getByRole("radio", { name: "有预占" })).toBeDefined()
        expect(
            screen.getByText("将同时应用上方关键词和以下筛选条件；结果也用于导出。"),
        ).toBeDefined()

        const submit = container.querySelector('form button[type="submit"]')
        expect(submit?.textContent).toBe("应用全部筛选")
    })

    it("submits the single form through the shared apply path", () => {
        const { container, props } = renderToolbar({ open: true })

        fireEvent.submit(container.querySelector("form")!)
        expect(props.applyFilters).toHaveBeenCalledTimes(1)
    })

    it("toggles the more-filters panel with aria-expanded and aria-controls", () => {
        const { props } = renderToolbar()

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.getAttribute("aria-controls")).toBeTruthy()
        fireEvent.click(toggle)
        expect(props.setPanelOpen).toHaveBeenCalled()
    })

    it("labels the panel uniquely and links it from the toggle", () => {
        const { container } = renderToolbar({ open: true })

        const panel = screen.getByLabelText("库存台账更多筛选条件")
        expect(panel).toBeDefined()
        expect(container.querySelectorAll('[aria-label="库存台账更多筛选条件"]')).toHaveLength(1)
        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-controls")).toBe(panel.id)
    })

    it("shows movement-view fields only on the movement view", () => {
        renderToolbar({ view: "movement", open: true })

        expect(screen.getByLabelText("流水类型")).toBeDefined()
        expect(screen.getByLabelText("发生日期起")).toBeDefined()
        expect(screen.getByLabelText("发生日期止")).toBeDefined()
        expect(screen.queryByRole("radio", { name: "有可用" })).toBeNull()

        cleanup()
        renderToolbar({ view: "reservation", open: true })
        expect(screen.queryByLabelText("流水类型")).toBeNull()
        expect(screen.queryByLabelText("发生日期起")).toBeNull()
        expect(screen.getByLabelText("仓库")).toBeDefined()
    })

    it("shows applied conditions as removable chips with 清空全部", () => {
        const { props } = renderToolbar({
            chips: [
                { key: "q", label: "搜索：SKU-1" },
                { key: "warehouseId", label: "仓库：WH01" },
            ],
            hasActiveFilters: true,
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：SKU-1")).toBeDefined()
        expect(screen.getByText("仓库：WH01")).toBeDefined()
        expect(
            screen.getByRole("button", { name: "清空全部" }),
        ).toBeDefined()

        fireEvent.click(screen.getByRole("button", { name: "移除搜索：SKU-1" }))
        expect(props.removeFilter).toHaveBeenCalledWith("q")
        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(props.clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it("does not render the chip row when nothing is applied", () => {
        renderToolbar()

        expect(screen.queryByText("已筛选")).toBeNull()
        expect(
            screen.queryByRole("button", { name: "清空全部" }),
        ).toBeNull()
    })

    it("links the date range error with role=alert and aria-invalid/aria-describedby", () => {
        renderToolbar({
            view: "movement",
            open: true,
            filterError: "截止日期不能早于起始日期",
        })

        const alert = screen.getByRole("alert")
        expect(alert.textContent).toBe("截止日期不能早于起始日期")
        const from = screen.getByLabelText("发生日期起")
        const to = screen.getByLabelText("发生日期止")
        expect(from.getAttribute("aria-invalid")).toBe("true")
        expect(to.getAttribute("aria-invalid")).toBe("true")
        expect(from.getAttribute("aria-describedby")).toBe(alert.id)
        expect(to.getAttribute("aria-describedby")).toBe(alert.id)
    })
})
