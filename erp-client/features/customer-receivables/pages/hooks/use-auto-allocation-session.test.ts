import { cleanup, renderHook, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"
import { useAutoAllocationSession } from "./use-auto-allocation-session"
import type { CustomerAccountsListView } from "../../types"

afterEach(cleanup)
const args = () => ({
    data: {
        canRegister: true,
        receivables: [
            {
                counterpartyPartyId: "p",
                counterpartyPartyName: "主体甲",
                salesOrderId: "so-1",
                accountId: "a",
            },
        ],
        counterparties: [],
    } as unknown as CustomerAccountsListView,
    from: "W05",
    returnTo: "/sales/orders/so-1?section=receivable",
    sessionId: undefined,
    counterpartyPartyId: undefined,
    customerId: undefined,
    salesOrderId: "so-1",
    registerMode: undefined,
    receivableAccountId: undefined,
    canRegister: true,
    createSession: {
        mutateAsync: vi.fn().mockResolvedValue({ draftSessionId: "session-1" }),
    },
    patchUrl: vi.fn(),
    setActionError: vi.fn(),
})

it("打开客户往来仅保留销售单范围，不因 W05 来源自动开始登记", () => {
    const input = args()
    renderHook(() => useAutoAllocationSession(input))
    expect(input.createSession.mutateAsync).not.toHaveBeenCalled()
    expect(input.patchUrl).not.toHaveBeenCalled()
})

it("明确指定回款登记时保留原有会话与返回上下文", async () => {
    const input = { ...args(), registerMode: "receipt" as const }
    renderHook(() => useAutoAllocationSession(input))
    await waitFor(() =>
        expect(input.patchUrl).toHaveBeenCalledWith(
            { sessionId: "session-1" },
            { replace: true },
        ),
    )
    expect(input.createSession.mutateAsync).toHaveBeenCalledWith(
        expect.objectContaining({
            mode: "receipt",
            salesOrderId: "so-1",
            returnTo: input.returnTo,
        }),
    )
})

it("显式登记链接不能绕过办理权限", () => {
    const input = {
        ...args(),
        registerMode: "receipt" as const,
        canRegister: false,
    }
    renderHook(() => useAutoAllocationSession(input))
    expect(input.createSession.mutateAsync).not.toHaveBeenCalled()
})
