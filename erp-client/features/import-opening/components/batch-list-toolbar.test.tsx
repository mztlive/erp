import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ComponentProps } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
    BatchListToolbar,
    type BatchAppliedChip,
} from "./batch-list-toolbar"

type ToolbarProps = ComponentProps<typeof BatchListToolbar>

afterEach(cleanup)

const noop = vi.fn()

function renderToolbar(overrides: Partial<ToolbarProps> = {}) {
    const props: ToolbarProps = {
        searchInputRef: { current: null },
        searchDraft: "",
        setSearchDraft: noop,
        hasActiveFilters: false,
        clearAllFilters: noop,
        appliedChips: [],
        removeFilter: noop,
        batchFilterPanelOpen: false,
        setBatchFilterPanelOpen: noop,
        hasStructuredBatchFilters: false,
        applyBatchFilters: noop,
        resetMoreFilters: noop,
        objectTypeDraft: "all",
        setObjectTypeDraft: noop,
        statusDraft: "all",
        setStatusDraft: noop,
        ...overrides,
    }
    return render(<BatchListToolbar {...props} />)
}

describe("BatchListToolbar", () => {
    it("renders a single form with no inline submit button at the search box tail", () => {
        const { container } = renderToolbar()

        expect(container.querySelectorAll("form")).toHaveLength(1)
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        // 收起态没有独立「搜索」按钮，也没有面板主提交。
        expect(screen.queryByRole("button", { name: "搜索" })).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("hides the tail arrow and keeps a single apply button while the panel is open", () => {
        renderToolbar({ batchFilterPanelOpen: true })

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).toBeDefined()
    })

    it("submits the same apply function from the form-level submit", () => {
        const applyBatchFilters = vi.fn()
        const { container } = renderToolbar({ applyBatchFilters })

        fireEvent.submit(container.querySelector("form")!)

        expect(applyBatchFilters).toHaveBeenCalledTimes(1)
    })

    it("shows applied chips in a dedicated row ending with 清空全部", () => {
        const removeFilter = vi.fn()
        const clearAllFilters = vi.fn()
        renderToolbar({
            hasActiveFilters: true,
            appliedChips: [
                { key: "q", label: "搜索：B-01" },
                { key: "status", label: "状态：失败" },
            ],
            removeFilter,
            clearAllFilters,
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：B-01")).toBeDefined()
        expect(screen.getByText("状态：失败")).toBeDefined()

        fireEvent.click(screen.getByRole("button", { name: "移除搜索：B-01" }))
        expect(removeFilter).toHaveBeenCalledWith("q")

        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it("keeps chips visible while the panel is closed", () => {
        renderToolbar({
            hasActiveFilters: true,
            appliedChips: [
                { key: "objectType", label: "对象：客户" } satisfies BatchAppliedChip,
            ],
        })

        expect(screen.getByText("对象：客户")).toBeDefined()
        expect(screen.queryByLabelText("导入批次更多筛选条件")).toBeNull()
    })

    it("toggles the panel with a type=button carrying aria-expanded and aria-controls", () => {
        const setBatchFilterPanelOpen = vi.fn()
        renderToolbar({ setBatchFilterPanelOpen })

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("type")).toBe("button")
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.getAttribute("aria-controls")).toBeTruthy()

        fireEvent.click(toggle)
        expect(setBatchFilterPanelOpen).toHaveBeenCalledTimes(1)
        expect(typeof setBatchFilterPanelOpen.mock.calls[0]![0]).toBe(
            "function",
        )
    })

    it("exposes the panel as a labelled region with a single primary submit", () => {
        renderToolbar({ batchFilterPanelOpen: true })

        const panel = screen.getByLabelText("导入批次更多筛选条件")
        expect(panel.id).toBe(
            screen
                .getByRole("button", { name: /更多筛选/ })
                .getAttribute("aria-controls"),
        )
        // 面板内只有底部一个主提交。
        expect(
            screen.getAllByRole("button", { name: "应用全部筛选" }),
        ).toHaveLength(1)
        expect(screen.getByText("对象集合")).toBeDefined()
        expect(screen.getByText("批次状态")).toBeDefined()
    })

    it("marks the more-filters toggle as enabled when structured filters are applied", () => {
        renderToolbar({ hasStructuredBatchFilters: true })

        expect(screen.getByText("已启用")).toBeDefined()
    })

    it("clears structured drafts from the panel without leaving the form", () => {
        const resetMoreFilters = vi.fn()
        renderToolbar({ batchFilterPanelOpen: true, resetMoreFilters })

        fireEvent.click(screen.getByRole("button", { name: "重置更多条件" }))
        expect(resetMoreFilters).toHaveBeenCalledTimes(1)
    })
})
