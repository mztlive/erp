import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { DictionaryListToolbar } from "./dictionary-list-toolbar"

afterEach(cleanup)

function renderToolbar(open: boolean, chips: unknown[] = []) {
    const noop = vi.fn()
    return render(
        <DictionaryListToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            searchPlaceholder="单位代码、名称、符号"
            countLabel="计量单位"
            hasActiveFilters={chips.length > 0}
            clearAllFilters={noop}
            appliedChips={chips as never}
            removeFilter={noop}
            filterPanelOpen={open}
            setFilterPanelOpen={noop}
            hasStructuredListFilters={false}
            applyListFilters={noop}
            resetMoreFilters={noop}
            lifecycleStatusDraft="all"
            setLifecycleStatusDraft={noop}
            revisionTimingDraft="all"
            setRevisionTimingDraft={noop}
        />,
    )
}

describe("DictionaryListToolbar", () => {
    it("does not show an inline search submit while more filters are closed", () => {
        renderToolbar(false)

        expect(
            screen.queryByRole("button", { name: "搜索" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("shows only the explicit apply action while more filters are open", () => {
        renderToolbar(true)

        expect(
            screen.queryByRole("button", { name: "搜索" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
    })

    it("wires the more-filters toggle with aria-expanded and aria-controls", () => {
        renderToolbar(false)

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.hasAttribute("aria-controls")).toBe(true)
        expect(
            document.getElementById(String(toggle.getAttribute("aria-controls"))),
        ).toBeNull()
    })

    it("renders applied chips with clear-all in the secondary row", () => {
        renderToolbar(false, [
            { key: "q", label: "搜索：箱" },
            { key: "lifecycleStatus", label: "启停：当前启用" },
        ])

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByRole("button", { name: "清空全部" })).toBeDefined()
        expect(screen.getByText("搜索：箱")).toBeDefined()
        expect(screen.getByText("启停：当前启用")).toBeDefined()
    })
})
