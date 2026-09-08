import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { PurchaseOrderCreatePreviewDialog } from "./purchase-order-create-preview"
beforeEach(() => {
    vi.stubGlobal(
        "ResizeObserver",
        class {
            observe() {}
            unobserve() {}
            disconnect() {}
        },
    )
})
afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
})
const stock = [
    {
        salesOrderLineId: "line-1",
        itemName: "礼盒",
        warehouseName: "主仓",
        quantity: "2",
        unit: "件",
    },
]
describe("sourcing final preview", () => {
    it("runs the final submit directly and keeps the reviewed summary visible", () => {
        const submit = vi.fn()
        const close = vi.fn()
        render(
            <PurchaseOrderCreatePreviewDialog
                open
                previews={[]}
                stockAllocations={stock}
                description="库存分配 1 条，共 2 件"
                onConfirm={submit}
                onOpenChange={close}
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "确认库存分配" }))
        expect(submit).toHaveBeenCalledTimes(1)
        expect(close).not.toHaveBeenCalled()
        expect(screen.getByText("库存分配 1 条，共 2 件")).toBeTruthy()
        expect(screen.getAllByRole("dialog")).toHaveLength(1)
    })
    it("keeps an unknown command recoverable even if refreshed previews are empty", () => {
        const submit = vi.fn()
        const close = vi.fn()
        render(
            <PurchaseOrderCreatePreviewDialog
                open
                unresolved
                previews={[]}
                stockAllocations={[]}
                onConfirm={submit}
                onOpenChange={close}
            />,
        )
        expect(
            (
                screen.getByRole("button", {
                    name: "返回编辑",
                }) as HTMLButtonElement
            ).disabled,
        ).toBe(true)
        expect(screen.queryByRole("button", { name: "关闭" })).toBeNull()
        fireEvent.click(screen.getByRole("button", { name: "核对提交结果" }))
        expect(submit).toHaveBeenCalledTimes(1)
        expect(close).not.toHaveBeenCalled()
    })
})
