import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SupplierAccountsViewTabs } from "./supplier-accounts-view-tabs"

afterEach(() => {
    cleanup()
})

describe("SupplierAccountsViewTabs", () => {
    it("高亮当前工作视图", () => {
        render(
            <SupplierAccountsViewTabs view="payable" onViewChange={() => {}} />,
        )

        expect(
            screen
                .getByRole("tab", { name: "应付台账" })
                .getAttribute("aria-selected"),
        ).toBe("true")
        expect(
            screen
                .getByRole("tab", { name: "付款" })
                .getAttribute("aria-selected"),
        ).toBe("false")
    })

    it("点击其他视图时通知切换", () => {
        const onViewChange = vi.fn()
        render(
            <SupplierAccountsViewTabs
                view="payable"
                onViewChange={onViewChange}
            />,
        )

        fireEvent.click(screen.getByRole("tab", { name: "待核销" }))

        expect(onViewChange).toHaveBeenCalledWith("unallocated")
    })
})
