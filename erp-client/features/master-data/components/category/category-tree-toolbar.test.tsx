import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { CategoryTreeToolbar } from "./category-tree-toolbar"

afterEach(cleanup)

function renderToolbar(chips: unknown[] = []) {
    const noop = vi.fn()
    return render(
        <CategoryTreeToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            applyTreeFilters={noop}
            lifecycleStatus="all"
            onLifecycleStatusChange={noop}
            appliedChips={chips as never}
            removeFilter={noop}
            clearFilters={noop}
        />,
    )
}

describe("CategoryTreeToolbar", () => {
    it("submits the search draft through a single form without an inline search button", () => {
        renderToolbar()

        expect(
            screen.queryByRole("button", { name: "搜索" }),
        ).toBeNull()
        expect(
            screen.getByRole("textbox", { name: "搜索分类代码或名称" }),
        ).toBeDefined()
    })

    it("marks the active lifecycle quick filter with aria-pressed", () => {
        const noop = vi.fn()
        render(
            <CategoryTreeToolbar
                searchInputRef={{ current: null }}
                searchDraft=""
                setSearchDraft={noop}
                applyTreeFilters={noop}
                lifecycleStatus="enabled"
                onLifecycleStatusChange={noop}
                appliedChips={[]}
                removeFilter={noop}
                clearFilters={noop}
            />,
        )

        const enabled = screen.getByRole("button", { name: "当前启用" })
        const all = screen.getByRole("button", { name: "全部" })
        expect(enabled.getAttribute("aria-pressed")).toBe("true")
        expect(all.getAttribute("aria-pressed")).toBe("false")
    })

    it("renders applied chips with clear-all in the secondary row", () => {
        renderToolbar([
            { key: "q", label: "搜索：甲" },
            { key: "lifecycleStatus", label: "启停：当前启用" },
        ])

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByRole("button", { name: "清空全部" })).toBeDefined()
        expect(screen.getByText("搜索：甲")).toBeDefined()
    })
})
