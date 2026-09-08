import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"

afterEach(cleanup)

describe("FormalActionConfirmDialog", () => {
    it("keeps normal submission styling independent of warning text and preserves failure", async () => {
        const close = vi.fn()
        const submit = vi.fn().mockRejectedValue(new Error("版本已变化"))
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={close}
                actionLabel="提交审批"
                confirmLabel="提交审批"
                actionVariant="default"
                summary={["采购单 CG-001", "金额 120.00"]}
                irreversibleEffects={["审批通过后更新库存"]}
                fromStatus={{ label: "草稿" }}
                toStatus={{ label: "审批中" }}
                onConfirm={submit}
            />,
        )
        expect(screen.getByText("核对信息")).toBeTruthy()
        expect(screen.queryByText("提交后锁定字段")).toBeNull()
        const button = screen.getByRole("button", { name: "提交审批" })
        expect(button.className).not.toContain("bg-destructive")
        fireEvent.click(button)
        await waitFor(() => expect(screen.getByText("版本已变化")).toBeTruthy())
        expect(close).not.toHaveBeenCalledWith(false)
    })
    it("uses a block container when the description contains structured content", () => {
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={() => undefined}
                actionLabel="提交采购审批"
                fromStatus={{ label: "草稿", tone: "neutral" }}
                toStatus={{ label: "审批中", tone: "warning" }}
                description={
                    <div>
                        <p>确认后启动审批。</p>
                        <section>审批路线</section>
                    </div>
                }
                onConfirm={() => undefined}
            />,
        )

        const description = document.querySelector(
            '[data-slot="alert-dialog-description"]',
        )
        expect(description?.tagName).toBe("DIV")
        expect(description?.querySelector("p")?.parentElement).toBe(
            description?.firstElementChild,
        )
        expect(description?.querySelector("p p")).toBeNull()
    })

    it("keeps status change in the header text column", () => {
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={() => undefined}
                title="确认发货？"
                actionLabel="确认发货"
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{ label: "已发货", tone: "success" }}
                onConfirm={() => undefined}
            />,
        )

        const header = document.querySelector(
            '[data-slot="alert-dialog-header"]',
        )
        const status = document.querySelector('[aria-label="状态变化"]')
        expect(header?.contains(status)).toBe(true)
        expect(status?.parentElement?.className).toContain("col-start-2")
    })
})
