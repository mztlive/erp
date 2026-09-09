import { cleanup, renderHook } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"
import { useCustomerReceivablesPreview } from "./use-customer-receivables-preview"

afterEach(cleanup)
it("同一页面原单链接改变 URL 后，预览切到原单而非保留当前冲正", () => {
    const patchUrl = vi.fn()
    const initialProps: Parameters<typeof useCustomerReceivablesPreview>[0] = {
        view: "receipt",
        previewKind: "reversal",
        previewId: "reversal-1",
        focusId: undefined,
        patchUrl,
    }
    const { result, rerender } = renderHook(useCustomerReceivablesPreview, {
        initialProps,
    })
    expect(result.current.preview).toEqual({
        kind: "reversal",
        id: "reversal-1",
    })
    rerender({
        ...initialProps,
        previewKind: "receipt",
        previewId: "receipt-original",
    })
    expect(result.current.preview).toEqual({
        kind: "receipt",
        id: "receipt-original",
    })
    expect(patchUrl).not.toHaveBeenCalled()
})
it("返回不带预览的 URL 时关闭当前原单", () => {
    const initialProps: Parameters<typeof useCustomerReceivablesPreview>[0] = {
        view: "sales_invoice",
        previewKind: "invoice",
        previewId: "invoice-original",
        focusId: undefined,
        patchUrl: vi.fn(),
    }
    const { result, rerender } = renderHook(useCustomerReceivablesPreview, {
        initialProps,
    })
    rerender({ ...initialProps, previewKind: null, previewId: undefined })
    expect(result.current.preview).toBeNull()
})
