import * as React from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
    CustomerQualityFilterCard,
    type CustomerQualityFilterCardProps,
} from "./customer-quality-filter-card"

function baseProps(
    overrides: Partial<CustomerQualityFilterCardProps> = {},
): CustomerQualityFilterCardProps {
    return {
        searchDraft: "",
        onSearchDraftChange: vi.fn(),
        searchInputRef: React.createRef<HTMLInputElement | null>(),
        panelOpen: false,
        setPanelOpen: vi.fn(),
        hasStructuredFilters: false,
        appliedChips: [],
        onRemoveFilter: vi.fn(),
        onApplyFilters: vi.fn(),
        onClearAllFilters: vi.fn(),
        onResetMoreFilters: vi.fn(),
        fundsReviewDraft: "all",
        setFundsReviewDraft: vi.fn(),
        businessTypeDraft: "all",
        setBusinessTypeDraft: vi.fn(),
        ...overrides,
    }
}

afterEach(() => {
    cleanup()
})

describe("CustomerQualityFilterCard — single form and submit paths", () => {
    it("renders one form with no inline submit button and no panel button", () => {
        const { container } = render(
            <CustomerQualityFilterCard {...baseProps()} />,
        )

        expect(container.querySelectorAll("form").length).toBe(1)
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: /更多筛选/ }).getAttribute(
                "aria-expanded",
            ),
        ).toBe("false")
    })

    it("submits through the form-level submit and the panel button with the same apply function", () => {
        const onApplyFilters = vi.fn()
        const props = baseProps({ onApplyFilters })
        const { container, rerender } = render(
            <CustomerQualityFilterCard {...props} />,
        )

        fireEvent.submit(container.querySelector("form")!)
        expect(onApplyFilters).toHaveBeenCalledTimes(1)

        rerender(<CustomerQualityFilterCard {...props} panelOpen />)
        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        fireEvent.click(
            screen.getByRole("button", { name: "应用全部筛选" }),
        )
        expect(onApplyFilters).toHaveBeenCalledTimes(2)
    })

    it("submits the same apply on a form-level submit (Enter path)", () => {
        const onApplyFilters = vi.fn()
        const { container } = render(
            <CustomerQualityFilterCard
                {...baseProps({ onApplyFilters })}
            />,
        )

        fireEvent.submit(container.querySelector("form")!)
        expect(onApplyFilters).toHaveBeenCalledTimes(1)
    })

    it("keeps the panel inside the same form with a unique id and aria wiring", () => {
        const setPanelOpen = vi.fn()
        const { container } = render(
            <CustomerQualityFilterCard
                {...baseProps({ setPanelOpen, panelOpen: true })}
            />,
        )

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("true")
        const controls = toggle.getAttribute("aria-controls")
        expect(controls).not.toBeNull()
        const panel = document.getElementById(controls!)
        expect(panel).not.toBeNull()
        expect(panel?.getAttribute("aria-label")).toBe(
            "客户经营质量更多筛选条件",
        )
        expect(container.querySelectorAll("form").length).toBe(1)
        expect(
            container.querySelectorAll('button[aria-expanded="true"]')
                .length,
        ).toBe(1)

        fireEvent.click(toggle)
        expect(setPanelOpen).toHaveBeenCalledTimes(1)
        expect(typeof setPanelOpen.mock.calls[0]![0]).toBe("function")
    })

    it("shows the bottom row with 重置更多条件 and 应用全部筛选 only when expanded", () => {
        const onResetMoreFilters = vi.fn()
        render(
            <CustomerQualityFilterCard
                {...baseProps({
                    panelOpen: true,
                    onResetMoreFilters,
                })}
            />,
        )

        expect(
            screen.getByText(
                "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
            ),
        ).not.toBeNull()
        fireEvent.click(
            screen.getByRole("button", { name: "重置更多条件" }),
        )
        expect(onResetMoreFilters).toHaveBeenCalledTimes(1)
    })
})

describe("CustomerQualityFilterCard — chips and structured fields", () => {
    it("shows applied chips with 已筛选 and 清空全部", () => {
        const onRemoveFilter = vi.fn()
        const onClearAllFilters = vi.fn()
        render(
            <CustomerQualityFilterCard
                {...baseProps({
                    appliedChips: [
                        { key: "q", label: "搜索：ABC" },
                        { key: "customerId", label: "客户：华东商贸" },
                    ],
                    onRemoveFilter,
                    onClearAllFilters,
                })}
            />,
        )

        expect(screen.getByText("已筛选")).not.toBeNull()
        expect(screen.getByText("搜索：ABC")).not.toBeNull()
        expect(screen.getByText("客户：华东商贸")).not.toBeNull()

        fireEvent.click(screen.getByRole("button", { name: "清空全部" }))
        expect(onClearAllFilters).toHaveBeenCalledTimes(1)

        fireEvent.click(
            screen.getByRole("button", { name: "移除搜索：ABC" }),
        )
        expect(onRemoveFilter).toHaveBeenCalledWith("q")
    })

    it("shows the 已启用 badge only for applied structured filters", () => {
        const { rerender } = render(
            <CustomerQualityFilterCard {...baseProps()} />,
        )
        expect(screen.queryByText("已启用")).toBeNull()

        rerender(
            <CustomerQualityFilterCard
                {...baseProps({ hasStructuredFilters: true })}
            />,
        )
        expect(screen.getByText("已启用")).not.toBeNull()
    })

    it("edits the structured drafts through the radio filters", () => {
        const setFundsReviewDraft = vi.fn()
        const setBusinessTypeDraft = vi.fn()
        render(
            <CustomerQualityFilterCard
                {...baseProps({
                    panelOpen: true,
                    setFundsReviewDraft,
                    setBusinessTypeDraft,
                })}
            />,
        )

        fireEvent.click(
            screen.getByRole("radio", { name: "仅已复核卡券票款" }),
        )
        expect(setFundsReviewDraft).toHaveBeenCalledWith("reviewed_only")

        fireEvent.click(screen.getByRole("radio", { name: "非卡券" }))
        expect(setBusinessTypeDraft).toHaveBeenCalledWith("GOODS_SERVICE")
    })

    it("removes a chip via its key through onRemoveFilter", () => {
        const onRemoveFilter = vi.fn()
        render(
            <CustomerQualityFilterCard
                {...baseProps({
                    appliedChips: [
                        { key: "fundsReview", label: "票款口径：仅已复核卡券票款" },
                    ],
                    onRemoveFilter,
                })}
            />,
        )

        fireEvent.click(
            screen.getByRole("button", {
                name: "移除票款口径：仅已复核卡券票款",
            }),
        )
        expect(onRemoveFilter).toHaveBeenCalledWith("fundsReview")
    })
})
