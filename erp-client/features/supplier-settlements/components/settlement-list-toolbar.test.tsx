import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    within,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { Mock } from "vitest"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import { SettlementListToolbar } from "./settlement-list-toolbar"

afterEach(cleanup)

function makeUrlState(
    overrides: Partial<SettlementsUrlState> = {},
): SettlementsUrlState {
    return {
        view: "pending",
        page: 1,
        section: "overview",
        ...overrides,
    }
}

function renderToolbar({
    open = false,
    urlState = makeUrlState(),
    applyFilters = vi.fn<() => void>(),
    clearAllFilters = vi.fn<() => void>(),
}: {
    open?: boolean
    urlState?: SettlementsUrlState
    applyFilters?: Mock<() => void>
    clearAllFilters?: Mock<() => void>
} = {}) {
    const noop = vi.fn()
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    return {
        applyFilters,
        clearAllFilters,
        ...render(
            <QueryClientProvider client={client}>
                <SettlementListToolbar
                    urlState={urlState}
                    suppliers={[
                        { supplierId: "sup1", supplierName: "上海材料供应" },
                    ]}
                    searchInputRef={{ current: null }}
                    searchDraft={urlState.q ?? ""}
                    setSearchDraft={noop}
                    panelOpen={open}
                    setPanelOpen={noop}
                    hasActiveFilters={Boolean(
                        urlState.q ||
                            urlState.supplierId ||
                            urlState.status ||
                            urlState.differenceType ||
                            urlState.periodFrom ||
                            urlState.periodTo,
                    )}
                    applyFilters={applyFilters}
                    removeFilter={noop}
                    resetMoreFilters={noop}
                    clearAllFilters={clearAllFilters}
                    supplierIdDraft={urlState.supplierId ?? null}
                    setSupplierIdDraft={noop}
                    statusDraft={
                        urlState.status
                            ? urlState.status.split(",")
                            : []
                    }
                    setStatusDraft={noop}
                    differenceTypeDraft={urlState.differenceType ?? "all"}
                    setDifferenceTypeDraft={noop}
                    periodFromDraft={urlState.periodFrom ?? ""}
                    setPeriodFromDraft={noop}
                    periodToDraft={urlState.periodTo ?? ""}
                    setPeriodToDraft={noop}
                    periodError={null}
                    setPeriodError={noop}
                />
            </QueryClientProvider>,
        ),
    }
}

describe("SettlementListToolbar", () => {
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
        expect(screen.getByText("将同时应用上方关键词和以下筛选条件；结果也用于导出。")).toBeDefined()
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

    it("shows applied conditions as chips with 清空全部", () => {
        renderToolbar({
            urlState: makeUrlState({
                q: "st-1",
                status: "DRAFT,PENDING_REVIEW",
                periodFrom: "2026-01-01",
            }),
        })

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：st-1")).toBeDefined()
        expect(screen.getByText("状态：草稿、待复核")).toBeDefined()
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

        expect(screen.getByText("供应商")).toBeDefined()
        expect(screen.getByText("状态")).toBeDefined()
        expect(screen.getByText("差异类型")).toBeDefined()
        expect(screen.getByText("结算期间")).toBeDefined()
        // 面板不再嵌套 Card：面板自身不渲染 data-slot=card
        expect(document.querySelector('[data-slot="card"]')).toBeNull()
    })
})
