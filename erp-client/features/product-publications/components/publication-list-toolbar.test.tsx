import * as React from "react"
import type { ComponentProps } from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
    PublicationListToolbar,
    type PublicationAppliedChip,
} from "./publication-list-toolbar"

afterEach(cleanup)

function toolbarProps({
    open = false,
    hasStructuredFilters = false,
    chips = [],
}: {
    open?: boolean
    hasStructuredFilters?: boolean
    chips?: readonly PublicationAppliedChip[]
} = {}): ComponentProps<typeof PublicationListToolbar> {
    return {
        searchInputRef: {
            current: null,
        } as React.RefObject<HTMLInputElement | null>,
        searchDraft: "",
        setSearchDraft: vi.fn(),
        appliedChips: chips,
        removeFilter: vi.fn(),
        clearAllFilters: vi.fn(),
        panelOpen: open,
        setPanelOpen: vi.fn(),
        hasStructuredFilters,
        applyFilters: vi.fn(),
        resetMoreFilters: vi.fn(),
        mallDraft: null,
        setMallDraft: vi.fn(),
        publicationStatusDraft: "all",
        setPublicationStatusDraft: vi.fn(),
        deliveryStatusDraft: "all",
        setDeliveryStatusDraft: vi.fn(),
    }
}

describe("PublicationListToolbar", () => {
    it("wraps the whole filter area in a single form", () => {
        const { container } = render(
            <PublicationListToolbar {...toolbarProps({ open: true })} />,
        )

        expect(container.querySelectorAll("form")).toHaveLength(1)
    })

    it("does not render a submit arrow or a separate search button while the panel is closed", () => {
        render(<PublicationListToolbar {...toolbarProps()} />)

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: /更多筛选/ }),
        ).toBeDefined()
    })

    it("exposes the panel toggle with aria-expanded and aria-controls", () => {
        const { container } = render(
            <PublicationListToolbar {...toolbarProps({ open: true })} />,
        )

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        const panel = container.querySelector(
            '[aria-label="商品发布更多筛选条件"]',
        )

        expect(toggle.getAttribute("aria-expanded")).toBe("true")
        expect(toggle.getAttribute("aria-controls")).toBeTruthy()
        expect(panel).not.toBeNull()
        expect(panel?.id).toBe(toggle.getAttribute("aria-controls"))
    })

    it("keeps structured fields inside the collapsed panel", () => {
        const { container } = render(<PublicationListToolbar {...toolbarProps()} />)

        expect(screen.queryByText("发送状态")).toBeNull()
        expect(screen.queryByText("目标商城")).toBeNull()
        expect(screen.queryByText("发布状态")).toBeNull()
        expect(
            container.querySelector('[aria-label="商品发布更多筛选条件"]'),
        ).toBeNull()
    })

    it("shows the fixed radio row, grid fields and the single main submit inside the open panel", () => {
        const { container } = render(
            <PublicationListToolbar {...toolbarProps({ open: true })} />,
        )

        expect(screen.getByText("发送状态")).toBeDefined()
        expect(screen.getByText("目标商城")).toBeDefined()
        expect(screen.getByText("发布状态")).toBeDefined()
        expect(screen.getByText("待商城确认")).toBeDefined()
        expect(
            screen.getByText("将同时应用上方关键词和以下筛选条件；结果也用于导出。"),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        // 面板是筛选卡内部第二层，不嵌套 Card
        expect(container.querySelector('[aria-label="商品发布更多筛选条件"] [data-slot="card"]')).toBeNull()
    })

    it("marks the toggle with the enabled badge only when structured filters are applied", () => {
        render(
            <PublicationListToolbar
                {...toolbarProps({ hasStructuredFilters: true })}
            />,
        )
        expect(screen.getByText("已启用")).toBeDefined()
    })

    it("submits the form through the shared apply callback", () => {
        const props = toolbarProps({ open: true })
        const { container } = render(<PublicationListToolbar {...props} />)

        fireEvent.submit(container.querySelector("form")!)

        expect(props.applyFilters).toHaveBeenCalledTimes(1)
    })

    it("renders every applied condition as a removable chip with a clear-all action", () => {
        const props = toolbarProps({
            chips: [
                { key: "q", label: "搜索：shoe" },
                { key: "metric", label: "指标：已暂停" },
            ],
        })
        render(<PublicationListToolbar {...props} />)

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：shoe")).toBeDefined()
        expect(screen.getByText("指标：已暂停")).toBeDefined()
        expect(screen.getByRole("button", { name: "清空全部" })).toBeDefined()

        fireEvent.click(screen.getByRole("button", { name: "移除搜索：shoe" }))
        expect(props.removeFilter).toHaveBeenCalledWith("q")
    })

    it("keeps the chip row visible while the panel is closed", () => {
        render(
            <PublicationListToolbar
                {...toolbarProps({
                    chips: [{ key: "q", label: "搜索：shoe" }],
                })}
            />,
        )

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByText("搜索：shoe")).toBeDefined()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })
})
