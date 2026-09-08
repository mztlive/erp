import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { FinancialRequestDialog } from "@/components/business/financial-request-dialog"
import { SubmissionResultUnknownError } from "@/lib/submission-result"

afterEach(cleanup)
const base = {
    open: true,
    pending: false,
    sourceLabel: "FK-2026-001 · 测试供应商",
    amount: "128.50",
    title: "供应商退款",
    description: "审批通过后登记供应商退款，原记录保留。",
    submitLabel: "提交退款审批",
    id: "refund-test",
}

describe("financial request single confirmation", () => {
    it("does not write on cancellation, requires a reason and submits once", async () => {
        const onSubmit = vi.fn().mockResolvedValue(undefined)
        const close = vi.fn()
        render(
            <FinancialRequestDialog
                {...base}
                onSubmit={onSubmit}
                onOpenChange={close}
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "取消" }))
        expect(close).toHaveBeenCalledWith(false)
        expect(onSubmit).not.toHaveBeenCalled()
        fireEvent.submit(document.querySelector("form")!)
        await waitFor(() => expect(screen.getByText("请填写原因")).toBeTruthy())
        expect(onSubmit).not.toHaveBeenCalled()
        fireEvent.change(document.getElementById("refund-test-reason")!, {
            target: { value: " 重复付款退款 " },
        })
        fireEvent.submit(document.querySelector("form")!)
        await waitFor(() =>
            expect(onSubmit).toHaveBeenCalledWith("重复付款退款"),
        )
        expect(onSubmit).toHaveBeenCalledTimes(1)
    })
    it("keeps failed input; unknown results lock it and retry the same intent", async () => {
        const onSubmit = vi
            .fn()
            .mockRejectedValueOnce(new Error("审批流程尚未配置"))
            .mockRejectedValueOnce(
                new SubmissionResultUnknownError("正在核对结果"),
            )
            .mockResolvedValue(undefined)
        const close = vi.fn()
        render(
            <FinancialRequestDialog
                {...base}
                onSubmit={onSubmit}
                onOpenChange={close}
            />,
        )
        const reason = document.getElementById(
            "refund-test-reason",
        ) as HTMLTextAreaElement
        fireEvent.change(reason, { target: { value: "退回重复付款" } })
        fireEvent.submit(document.querySelector("form")!)
        await screen.findByText("审批流程尚未配置")
        expect(reason.value).toBe("退回重复付款")
        expect(reason.disabled).toBe(false)
        fireEvent.submit(document.querySelector("form")!)
        await screen.findByText("提交结果尚未确定，请保持原内容并核对结果。")
        expect(reason.disabled).toBe(true)
        expect(
            (screen.getByRole("button", { name: "取消" }) as HTMLButtonElement)
                .disabled,
        ).toBe(true)
        fireEvent.click(screen.getByRole("button", { name: "核对提交结果" }))
        await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(3))
        expect(onSubmit.mock.calls.map((call) => call[0])).toEqual([
            "退回重复付款",
            "退回重复付款",
            "退回重复付款",
        ])
        expect(close).not.toHaveBeenCalled()
    })
})
