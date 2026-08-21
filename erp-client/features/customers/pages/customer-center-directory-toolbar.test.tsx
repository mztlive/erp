import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { CustomerAppliedChip } from "@/features/customers/hooks/use-customer-center-directory-state"
import { CustomerCenterDirectoryToolbar } from "./customer-center-directory-toolbar"

afterEach(cleanup)

function renderToolbar({
    open = false,
    chips = [],
    hasActiveFilters = false,
}: {
    open?: boolean
    chips?: readonly CustomerAppliedChip[]
    hasActiveFilters?: boolean
} = {}) {
    const props = {
        searchInputRef: { current: null },
        searchDraft: "",
        setSearchDraft: vi.fn(),
        scope: "mine" as const,
        onScopeChange: vi.fn(),
        statusDraft: "active" as const,
        setStatusDraft: vi.fn(),
        canReadAll: true,
        hasActiveFilters,
        appliedChips: chips,
        removeFilter: vi.fn(),
        panelOpen: open,
        setPanelOpen: vi.fn(),
        hasStructuredFilters: chips.some((chip) => chip.key === "status"),
        applyFilters: vi.fn(),
        resetMoreFilters: vi.fn(),
        clearAllFilters: vi.fn(),
    }
    const utils = render(<CustomerCenterDirectoryToolbar {...props} />)
    return { ...utils, props }
}

describe("CustomerCenterDirectoryToolbar", () => {
    it("renders a single semantic form for the whole filter area", () => {
        const { container } = renderToolbar({ open: true })

        expect(container.querySelectorAll("form")).toHaveLength(1)
        expect(container.querySelector("form")?.querySelector("input")).not.toBeNull()
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
        expect(screen.queryByRole("radio", { name: "启用" })).toBeNull()
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
        expect(screen.getByRole("radio", { name: "启用" })).toBeDefined()
        expect(screen.getByRole("radio", { name: "停用" })).toBeDefined()
        expect(
            screen.getByText("将同时应用上方关键词和以下筛选条件；结果也用于导出。"),
        ).toBeDefined()

        // 主提交按钮在唯一 form 内，通过 form submit 走 applyFilters
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

    it("shows applied conditions as removable chips with 清空全部", () => {
        const { props } = renderToolbar({
            chips: [
                { key: "q", label: "搜索：客户甲" },
                { key: "status", label: "状态：停用" },
            ],
            hasActiveFilters: true,
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：客户甲")).toBeDefined()
        expect(screen.getByText("状态：停用")).toBeDefined()
        expect(
            screen.getByRole("button", { name: "清空全部" }),
        ).toBeDefined()

        fireEvent.click(screen.getByRole("button", { name: "移除搜索：客户甲" }))
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
})
