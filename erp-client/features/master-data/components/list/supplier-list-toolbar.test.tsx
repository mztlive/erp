import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SupplierListToolbar } from "./supplier-list-toolbar"

afterEach(cleanup)

function renderToolbar(open: boolean, chips: unknown[] = []) {
    const noop = vi.fn()
    return render(
        <SupplierListToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            hasActiveFilters={chips.length > 0}
            clearAllFilters={noop}
            appliedChips={chips as never}
            removeFilter={noop}
            supplierFilterPanelOpen={open}
            setSupplierFilterPanelOpen={noop}
            hasStructuredSupplierFilters={false}
            applySupplierFilters={noop}
            resetMoreFilters={noop}
            lifecycleStatusDraft="all"
            setLifecycleStatusDraft={noop}
            supplierQualificationHealthDraft="all"
            setSupplierQualificationHealthDraft={noop}
            supplierCapabilityCodesDraft={[]}
            setSupplierCapabilityCodesDraft={noop}
            supplierQualificationTypesDraft={[]}
            setSupplierQualificationTypesDraft={noop}
        />,
    )
}

describe("SupplierListToolbar", () => {
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

    it("renders applied chips with clear-all in the secondary row", () => {
        renderToolbar(false, [
            { key: "q", label: "搜索：茶叶" },
            { key: "supplierCapabilityCodes", label: "供应能力：实物商品" },
        ])

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByRole("button", { name: "清空全部" })).toBeDefined()
        expect(screen.getByText("搜索：茶叶")).toBeDefined()
    })
})
