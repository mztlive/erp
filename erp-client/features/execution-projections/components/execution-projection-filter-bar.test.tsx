import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { ExecutionProjectionFilterBar } from "./execution-projection-filter-bar"
import type {
    ExecutionProjectionAppliedChip,
    ExecutionProjectionFilterState,
} from "@/features/execution-projections/hooks/use-execution-projection-filters"

afterEach(cleanup)

function makeFilters(
    overrides: Partial<ExecutionProjectionFilterState> = {},
) {
    const noop = vi.fn()
    return {
        searchInputRef: { current: null },
        searchDraft: "",
        setSearchDraft: noop,
        mallIdDraft: "all",
        setMallIdDraft: noop,
        deliveryStatusDraft: "all",
        setDeliveryStatusDraft: noop,
        latencyDraft: "all",
        setLatencyDraft: noop,
        reconciliationDraft: "all",
        setReconciliationDraft: noop,
        sourceDraft: "all",
        setSourceDraft: noop,
        panelOpen: false,
        setPanelOpen: noop,
        hasStructuredFilters: false,
        applyFilters: noop,
        clearAllFilters: noop,
        resetMoreFilters: noop,
        removeFilter: noop,
        ...overrides,
    } as unknown as ExecutionProjectionFilterState
}

function renderBar(
    overrides: Partial<ExecutionProjectionFilterState> = {},
    chips: readonly ExecutionProjectionAppliedChip[] = [],
) {
    const filters = makeFilters(overrides)
    render(
        <ExecutionProjectionFilterBar
            filters={filters}
            appliedChips={chips}
            removeFilter={filters.removeFilter}
            hasChips={chips.length > 0}
            malls={[
                { id: "m1", name: "华东商城" },
                { id: "m2", name: "华南商城" },
            ]}
        />,
    )
    return filters
}

describe("ExecutionProjectionFilterBar", () => {
    it("收起态搜索框尾部无提交箭头，且不出现「应用全部筛选」", () => {
        const filters = renderBar()
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        fireEvent.submit(document.querySelector("form")!)
        expect(filters.applyFilters).toHaveBeenCalledTimes(1)
    })

    it("展开态隐藏搜索框尾部箭头，只有面板底部唯一主提交", () => {
        const filters = renderBar({ panelOpen: true })
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        fireEvent.click(screen.getByRole("button", { name: "应用全部筛选" }))
        expect(filters.applyFilters).toHaveBeenCalledTimes(1)
    })

    it("收起态 Enter 与展开态主按钮走同一个 form 提交", () => {
        const filters = renderBar({ panelOpen: true })
        fireEvent.submit(document.querySelector("form")!)
        expect(filters.applyFilters).toHaveBeenCalledTimes(1)
    })

    it("「更多筛选」带 aria-expanded / aria-controls，点击只切展开态", () => {
        const filters = renderBar()
        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.hasAttribute("aria-controls")).toBe(true)
        fireEvent.click(toggle)
        expect(filters.setPanelOpen).toHaveBeenCalled()
    })

    it("展开面板含固定单选筛选与可搜索下拉，底部为范围说明与两个动作", () => {
        renderBar({ panelOpen: true })
        expect(screen.getByText("等待时长")).toBeDefined()
        expect(screen.getByText("版本核对")).toBeDefined()
        expect(screen.getByText("数据来源")).toBeDefined()
        expect(screen.getByText("目标商城")).toBeDefined()
        expect(screen.getByText("接收状态")).toBeDefined()
        expect(
            screen.getByText("将同时应用上方关键词和以下筛选条件；结果也用于导出。"),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
    })

    it("重置更多条件只重置结构化条件", () => {
        const filters = renderBar({ panelOpen: true })
        fireEvent.click(screen.getByRole("button", { name: "重置更多条件" }))
        expect(filters.resetMoreFilters).toHaveBeenCalledTimes(1)
    })

    it("已生效条件以 chip 行展示，末尾清空全部", () => {
        const filters = renderBar(
            { hasStructuredFilters: true },
            [
                { key: "q", label: "搜索：SO-1" },
                { key: "mall", label: "商城：华东商城" },
            ],
        )
        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：SO-1")).toBeDefined()
        expect(screen.getByText("商城：华东商城")).toBeDefined()
        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(filters.clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it("chip 关闭按钮只移除自己的条件", () => {
        const filters = renderBar(
            {},
            [{ key: "mall", label: "商城：华东商城" }],
        )
        fireEvent.click(
            screen.getByRole("button", { name: "移除商城：华东商城" }),
        )
        expect(filters.removeFilter).toHaveBeenCalledWith("mall")
    })

    it("无 chip 且面板收起时不渲染 secondary 行", () => {
        renderBar()
        expect(screen.queryByText("已筛选")).toBeNull()
        expect(screen.queryByText("清空全部")).toBeNull()
    })
})
