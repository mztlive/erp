import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { PurchaseOrdersListToolbar } from "./purchase-orders-list-toolbar"

afterEach(cleanup)

function renderToolbar(panelOpen: boolean) {
    const noop = vi.fn()
    return render(
        <PurchaseOrdersListToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            statusDraft="all"
            setStatusDraft={noop}
            panelOpen={panelOpen}
            setPanelOpen={noop}
            hasActiveFilters={false}
            hasStructuredFilters={false}
            appliedChips={[]}
            removeFilter={noop}
            applyFilters={noop}
            resetMoreFilters={noop}
            clearAllFilters={noop}
        />,
    )
}

describe("PurchaseOrdersListToolbar", () => {
    it("收起态没有内嵌提交箭头或独立搜索按钮", () => {
        renderToolbar(false)

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("展开态只保留「应用全部筛选」一个主提交", () => {
        renderToolbar(true)

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
    })

    it("整个筛选区只有一个表单，提交走同一个 apply 函数", () => {
        const applyFilters = vi.fn()
        const { container } = render(
            <PurchaseOrdersListToolbar
                searchInputRef={{ current: null }}
                searchDraft=""
                setSearchDraft={vi.fn()}
                statusDraft="all"
                setStatusDraft={vi.fn()}
                panelOpen={true}
                setPanelOpen={vi.fn()}
                hasActiveFilters={false}
                hasStructuredFilters={false}
                appliedChips={[]}
                removeFilter={vi.fn()}
                applyFilters={applyFilters}
                resetMoreFilters={vi.fn()}
                clearAllFilters={vi.fn()}
            />,
        )

        const forms = container.querySelectorAll("form")
        expect(forms).toHaveLength(1)
        fireEvent.submit(forms[0]!)
        expect(applyFilters).toHaveBeenCalledTimes(1)
    })

    it("已生效条件以 chip 展示并提供「清空全部」", () => {
        const clearAllFilters = vi.fn()
        render(
            <PurchaseOrdersListToolbar
                searchInputRef={{ current: null }}
                searchDraft=""
                setSearchDraft={vi.fn()}
                statusDraft="all"
                setStatusDraft={vi.fn()}
                panelOpen={false}
                setPanelOpen={vi.fn()}
                hasActiveFilters={true}
                hasStructuredFilters={false}
                appliedChips={[{ key: "q", label: "搜索：钢" }]}
                removeFilter={vi.fn()}
                applyFilters={vi.fn()}
                resetMoreFilters={vi.fn()}
                clearAllFilters={clearAllFilters}
            />,
        )

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：钢")).toBeDefined()
        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it("更多筛选按钮带 aria-expanded / aria-controls 且面板 id 唯一", () => {
        const { container } = renderToolbar(true)

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("true")
        const panelId = toggle.getAttribute("aria-controls")
        expect(panelId).toBeTruthy()
        const panel = container.querySelector(
            '[aria-label="采购单更多筛选条件"]',
        )
        expect(panel).not.toBeNull()
        expect(panel?.getAttribute("id")).toBe(panelId)
    })
})
