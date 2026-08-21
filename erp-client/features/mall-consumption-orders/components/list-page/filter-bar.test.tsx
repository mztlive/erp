import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    within,
} from "@testing-library/react"
import type { Dispatch, SetStateAction } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { Mock } from "vitest"

import type {
    MallConsumptionAppliedChip,
    MallConsumptionOrderFilterDraft,
} from "@/features/mall-consumption-orders/lib/filters"
import { EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT } from "@/features/mall-consumption-orders/lib/filters"
import { ConsumptionOrderFilterBar } from "./filter-bar"

afterEach(cleanup)

const noop = vi.fn()

function renderToolbar({
    open = false,
    chips = [],
    hasStructuredFilters = false,
    applyFilters = vi.fn<() => void>(),
    clearAllFilters = vi.fn<() => void>(),
    resetMoreFilters = vi.fn<() => void>(),
}: {
    open?: boolean
    chips?: readonly MallConsumptionAppliedChip[]
    hasStructuredFilters?: boolean
    applyFilters?: Mock<() => void>
    clearAllFilters?: Mock<() => void>
    resetMoreFilters?: Mock<() => void>
} = {}) {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    return {
        applyFilters,
        clearAllFilters,
        resetMoreFilters,
        ...render(
            <QueryClientProvider client={client}>
                <ConsumptionOrderFilterBar
                    searchInputRef={{ current: null }}
                    searchDraft=""
                    setSearchDraft={noop}
                    panelOpen={open}
                    setPanelOpen={noop}
                    hasStructuredFilters={hasStructuredFilters}
                    appliedChips={chips}
                    onRemoveFilter={noop}
                    onApplyFilters={applyFilters}
                    onClearAllFilters={clearAllFilters}
                    onResetMoreFilters={resetMoreFilters}
                    filterDraft={EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT}
                    setFilterDraft={
                        noop as unknown as Dispatch<
                            SetStateAction<MallConsumptionOrderFilterDraft>
                        >
                    }
                />
            </QueryClientProvider>,
        ),
    }
}

describe("ConsumptionOrderFilterBar", () => {
    it("renders the whole filter area inside one semantic form", () => {
        const { container } = renderToolbar()

        expect(container.querySelectorAll("form")).toHaveLength(1)
        expect(container.querySelector("form form")).toBeNull()
    })

    it("collapsed state has no submit button and no apply-all button", () => {
        renderToolbar()

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("hides the trailing arrow and shows only the panel apply button while expanded", () => {
        renderToolbar({ open: true })

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
    })

    it("submits the single form through the same apply function", () => {
        const { applyFilters, container } = renderToolbar({ open: true })

        fireEvent.submit(container.querySelector("form")!)

        expect(applyFilters).toHaveBeenCalledTimes(1)
    })

    it("marks the more-filters toggle with aria-expanded and aria-controls", () => {
        renderToolbar({ open: false })
        const toggle = screen.getByRole("button", { name: /更多筛选/ })

        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        const panelId = toggle.getAttribute("aria-controls")
        expect(panelId).toBeTruthy()

        const expanded = renderToolbar({ open: true })
        const expandedToggle = within(expanded.container).getByRole(
            "button",
            { name: /更多筛选/ },
        )
        expect(expandedToggle.getAttribute("aria-expanded")).toBe("true")
        expect(
            expanded.container.querySelector(
                `[id="${expandedToggle.getAttribute("aria-controls")}"]`,
            ),
        ).not.toBeNull()
    })

    it("shows the 已启用 badge only when structured filters are applied", () => {
        renderToolbar({ open: false, hasStructuredFilters: true })

        expect(screen.getByText("已启用")).toBeDefined()
    })

    it("shows applied conditions as chips with 清空全部", () => {
        renderToolbar({
            chips: [
                { key: "q", label: "搜索：st-1" },
                { key: "costBasis", label: "成本口径：无成本" },
            ],
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：st-1")).toBeDefined()
        expect(screen.getByText("成本口径：无成本")).toBeDefined()
        expect(
            screen.getByRole("button", { name: "清空全部" }),
        ).toBeDefined()
    })

    it("hides the chip row when nothing is applied", () => {
        renderToolbar()

        expect(screen.queryByText("已筛选")).toBeNull()
        expect(
            screen.queryByRole("button", { name: "清空全部" }),
        ).toBeNull()
    })

    it("keeps the panel fields inside the filter card as a plain second layer", () => {
        renderToolbar({ open: true })

        expect(screen.getByText("归集状态")).toBeDefined()
        expect(screen.getByText("履约链")).toBeDefined()
        expect(screen.getByText("支付方式")).toBeDefined()
        expect(screen.getByText("成本口径")).toBeDefined()
        expect(screen.getByText("事实类型")).toBeDefined()
        expect(screen.getByText("数据来源")).toBeDefined()
        expect(screen.getByText("来源商城")).toBeDefined()
        expect(screen.getByText("供应商状态")).toBeDefined()
        expect(screen.getByText("记录发生时间")).toBeDefined()
        // 面板不再嵌套 Card：面板自身不渲染 data-slot=card
        expect(document.querySelector('[data-slot="card"]')).toBeNull()
    })
})
