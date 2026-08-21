import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
    ConnectionListToolbar,
    type ConnectionListToolbarProps,
} from "./connection-list-toolbar"

vi.mock("@/features/entity-selectors", () => ({
    SupplierSearchCombobox: () => null,
}))

afterEach(cleanup)

const noop = vi.fn()

function renderToolbar(overrides: Partial<ConnectionListToolbarProps> = {}) {
    const props: ConnectionListToolbarProps = {
        searchInputRef: { current: null },
        searchDraft: "",
        onSearchDraftChange: noop,
        environment: "PRODUCTION",
        onEnvironmentChange: noop,
        filterPanelOpen: false,
        onFilterPanelOpenChange: noop,
        hasStructuredFilters: false,
        appliedChips: [],
        removeFilter: noop,
        onApplyFilters: noop,
        onClearFilters: noop,
        onResetMoreFilters: noop,
        statusDraft: "all",
        onStatusDraftChange: noop,
        healthDraft: [],
        onHealthDraftChange: noop,
        capabilityDraft: "",
        onCapabilityDraftChange: noop,
        catalogFreshnessDraft: [],
        onCatalogFreshnessDraftChange: noop,
        supplierIdDraft: null,
        onSupplierIdDraftChange: noop,
        ...overrides,
    }
    return render(<ConnectionListToolbar {...props} />)
}

describe("ConnectionListToolbar", () => {
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
        renderToolbar({ filterPanelOpen: true })

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
        const onApplyFilters = vi.fn()
        const { container } = renderToolbar({ onApplyFilters })

        fireEvent.submit(container.querySelector("form")!)

        expect(onApplyFilters).toHaveBeenCalledTimes(1)
    })

    it("shows applied chips in a dedicated row ending with 清空全部", () => {
        const removeFilter = vi.fn()
        const onClearFilters = vi.fn()
        renderToolbar({
            appliedChips: [
                { key: "q", label: "搜索：abc" },
                { key: "status", label: "状态：启用" },
            ],
            removeFilter,
            onClearFilters,
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：abc")).toBeDefined()
        expect(screen.getByText("状态：启用")).toBeDefined()

        fireEvent.click(screen.getByRole("button", { name: "移除搜索：abc" }))
        expect(removeFilter).toHaveBeenCalledWith("q")

        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(onClearFilters).toHaveBeenCalledTimes(1)
    })

    it("keeps applied chips visible while the panel is closed", () => {
        renderToolbar({
            appliedChips: [{ key: "supplierId", label: "供应商：供应商甲" }],
        })

        expect(screen.getByText("供应商：供应商甲")).toBeDefined()
        expect(
            screen.queryByLabelText("连接列表更多筛选条件"),
        ).toBeNull()
    })

    it("toggles the panel with a type=button carrying aria-expanded and aria-controls", () => {
        const onFilterPanelOpenChange = vi.fn()
        renderToolbar({ onFilterPanelOpenChange })

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.getAttribute("aria-controls")).toBeTruthy()

        fireEvent.click(toggle)
        expect(onFilterPanelOpenChange).toHaveBeenCalledWith(true)
    })

    it("exposes the panel as a labelled region with a single primary submit", () => {
        renderToolbar({ filterPanelOpen: true })

        const panel = screen.getByLabelText("连接列表更多筛选条件")
        expect(panel.id).toBe(
            screen
                .getByRole("button", { name: /更多筛选/ })
                .getAttribute("aria-controls"),
        )
        // 面板内只有底部一个主提交。
        expect(
            screen.getAllByRole("button", { name: "应用全部筛选" }),
        ).toHaveLength(1)
        // 结构化字段全部位于面板内。
        expect(screen.getByText("状态")).toBeDefined()
        expect(screen.getByText("供应商")).toBeDefined()
        expect(screen.getByText("能力")).toBeDefined()
        expect(screen.getByText("健康结果")).toBeDefined()
        expect(screen.getByText("目录更新时间")).toBeDefined()
    })

    it("marks 更多筛选 with 已启用 when structured filters are applied", () => {
        renderToolbar({ hasStructuredFilters: true })
        expect(screen.getByText("已启用")).toBeDefined()
    })

    it("renders the environment quick filter as a pressed segmented group", () => {
        const onEnvironmentChange = vi.fn()
        renderToolbar({
            environment: "STAGING",
            onEnvironmentChange,
        })

        const group = screen.getByRole("group", { name: "环境快捷筛选" })
        expect(group).toBeDefined()
        expect(
            screen.getByRole("button", { name: "全部" }).getAttribute("aria-pressed"),
        ).toBe("false")
        expect(
            screen
                .getByRole("button", { name: "测试" })
                .getAttribute("aria-pressed"),
        ).toBe("true")

        fireEvent.click(screen.getByRole("button", { name: "开发" }))
        expect(onEnvironmentChange).toHaveBeenCalledWith("DEVELOPMENT")
    })
})
