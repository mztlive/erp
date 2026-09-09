import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import { useCustomerDetailState } from "./use-customer-detail-state"

const mocks = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn() }))
vi.mock("next/navigation", () => ({ useRouter: () => mocks }))
vi.mock("@/features/customers/hooks/queries", () => ({
    useCustomerCenterQuery: () => ({ data: undefined }),
}))
vi.mock("@/components/ui/toast", () => ({ toast: { add: vi.fn() } }))

afterEach(cleanup)
beforeEach(() => vi.clearAllMocks())

test("未修改时返回客户列表", () => {
    const { result } = renderHook(() => useCustomerDetailState("customer-1"))
    act(() => result.current.handleBack())
    expect(mocks.push).toHaveBeenCalledWith("/sales/customers")
})

test("有未保存修改时拦截返回，取消后继续编辑，确认后才能离开", () => {
    const { result } = renderHook(() => useCustomerDetailState("customer-1"))
    act(() => {
        result.current.startEditing()
        result.current.setFormDirty(true)
    })
    act(() => result.current.handleBack())
    expect(mocks.push).not.toHaveBeenCalled()
    expect(result.current.pendingSection).toBe("back")
    act(() => result.current.dismissPendingSection())
    expect(result.current.editing).toBe(true)
    expect(result.current.formDirty).toBe(true)
    act(() => result.current.handleBack())
    act(() => result.current.discardPendingAndSwitch())
    expect(mocks.push).toHaveBeenCalledWith("/sales/customers")
    expect(result.current.pendingSection).toBeNull()
    expect(result.current.formDirty).toBe(false)
})

test("原有分区切换仍先确认再更新 URL", () => {
    const { result } = renderHook(() => useCustomerDetailState("customer-1"))
    act(() => {
        result.current.startEditing()
        result.current.setFormDirty(true)
    })
    act(() => result.current.handleSectionChange("related"))
    expect(mocks.replace).not.toHaveBeenCalled()
    act(() => result.current.discardPendingAndSwitch())
    expect(mocks.replace).toHaveBeenCalledWith(
        "/sales/customers/customer-1?section=related",
        { scroll: false },
    )
    expect(mocks.push).not.toHaveBeenCalled()
})
