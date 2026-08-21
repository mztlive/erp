import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ComponentProps } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { CustomerReceivablesToolbar } from "./customer-receivables-toolbar"

type CustomerReceivablesToolbarProps = ComponentProps<
    typeof CustomerReceivablesToolbar
>

vi.mock("@/features/entity-selectors", () => ({
    searchParties: vi.fn(async () => []),
    fetchPartyOption: vi.fn(async () => undefined),
    useDebouncedSearch: (input: string) => input,
}))

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(async () => ({ items: [], total: 0 })),
}))

const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
})

function renderToolbar(
    overrides: Partial<CustomerReceivablesToolbarProps> = {},
) {
    const props: CustomerReceivablesToolbarProps = {
        view: "receivable",
        searchDraft: "",
        setSearchDraft: vi.fn(),
        searchInputRef: { current: null },
        counterpartyPartyIdDraft: null,
        setCounterpartyPartyIdDraft: vi.fn(),
        dueDraft: "all",
        setDueDraft: vi.fn(),
        statusDraft: "all",
        setStatusDraft: vi.fn(),
        reviewStatusDraft: "all",
        setReviewStatusDraft: vi.fn(),
        panelOpen: false,
        setPanelOpen: vi.fn(),
        hasStructuredFilters: false,
        hasActiveFilters: false,
        appliedChips: [],
        removeFilter: vi.fn(),
        applyFilters: vi.fn(),
        resetMoreFilters: vi.fn(),
        clearFilters: vi.fn(),
        ...overrides,
    }
    const view = render(
        <QueryClientProvider client={queryClient}>
            <CustomerReceivablesToolbar {...props} />
        </QueryClientProvider>,
    )
    return { ...view, props }
}

afterEach(() => {
    cleanup()
    queryClient.clear()
})

describe("CustomerReceivablesToolbar", () => {
    it("renders a single semantic form carrying the ListToolbar", () => {
        renderToolbar()
        const forms = document.querySelectorAll("form")
        expect(forms).toHaveLength(1)
        expect(
            forms[0].contains(
                screen.getByRole("toolbar", { name: "列表工具栏" }),
            ),
        ).toBe(true)
    })

    it("collapsed state has no submit button, no search arrow and a type=button 更多筛选 toggle", () => {
        renderToolbar()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "清空全部" }),
        ).toBeNull()

        const more = screen.getByRole("button", { name: /更多筛选/ })
        expect(more.getAttribute("type")).toBe("button")
        expect(more.getAttribute("aria-expanded")).toBe("false")
        expect(more.getAttribute("aria-controls")).toBeTruthy()
    })

    it("Enter submit and the panel submit button share the same applyFilters through one form", () => {
        const applyFilters = vi.fn()
        renderToolbar({ applyFilters, panelOpen: true })

        const form = document.querySelector("form")
        expect(form).not.toBeNull()
        fireEvent.submit(form!)
        expect(applyFilters).toHaveBeenCalledTimes(1)

        const submit = screen.getByRole("button", { name: "应用全部筛选" })
        expect(submit.getAttribute("type")).toBe("submit")
        fireEvent.submit(form!)
        expect(applyFilters).toHaveBeenCalledTimes(2)
    })

    it("toggles the panel via 更多筛选 and links aria-controls to the panel", () => {
        const setPanelOpen = vi.fn()
        renderToolbar({ setPanelOpen, panelOpen: true })

        const more = screen.getByRole("button", { name: /更多筛选/ })
        expect(more.getAttribute("aria-expanded")).toBe("true")
        const panel = document.getElementById(
            more.getAttribute("aria-controls") ?? "",
        )
        expect(panel).not.toBeNull()
        expect(panel!.getAttribute("aria-label")).toBe("客户往来更多筛选条件")

        fireEvent.click(more)
        expect(setPanelOpen).toHaveBeenCalled()
    })

    it("shows receivable fields inside the panel with the fixed bottom action row", () => {
        renderToolbar({ panelOpen: true })
        expect(screen.getByText("往来主体")).toBeTruthy()
        expect(screen.getByText("到期")).toBeTruthy()
        expect(screen.getByText("状态")).toBeTruthy()
        expect(screen.getByText("复核状态")).toBeTruthy()
        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).toBeTruthy()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }).getAttribute(
                "type",
            ),
        ).toBe("button")
        expect(screen.getAllByRole("button", { name: "应用全部筛选" })).toHaveLength(1)
    })

    it("hides receivable-only fields on other views", () => {
        renderToolbar({ view: "receipt", panelOpen: true })
        expect(screen.getByText("往来主体")).toBeTruthy()
        expect(screen.queryByText("到期")).toBeNull()
        expect(screen.queryByText("状态")).toBeNull()
        expect(screen.queryByText("复核状态")).toBeNull()
    })

    it("renders applied conditions as removable chips with 清空全部", () => {
        const clearFilters = vi.fn()
        renderToolbar({
            hasActiveFilters: true,
            appliedChips: [
                { key: "q", label: "搜索：SO-1" },
                { key: "customerId", label: "经营客户 客户甲" },
            ],
            clearFilters,
        })

        expect(screen.getByText("已筛选")).toBeTruthy()
        expect(screen.getByText("搜索：SO-1")).toBeTruthy()
        expect(screen.getByText("经营客户 客户甲")).toBeTruthy()

        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(clearFilters).toHaveBeenCalledTimes(1)
    })

    it("resetMoreFilters is only inside the panel and keeps the panel open", () => {
        const resetMoreFilters = vi.fn()
        renderToolbar({ panelOpen: true, resetMoreFilters })

        fireEvent.click(
            screen.getByRole("button", { name: "重置更多条件" }),
        )
        expect(resetMoreFilters).toHaveBeenCalledTimes(1)
        // 面板仍在：主提交按钮未被卸载
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeTruthy()
    })
})
