import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { useSalesOrderCreateDefaults } from "./use-sales-order-create-defaults"
import { useSalesOrderCreateSubmission } from "./use-sales-order-create-submission"

const mutations = vi.hoisted(() => ({
    create: vi.fn(),
    save: vi.fn(),
    submit: vi.fn(),
}))
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn() }) }))
vi.mock("./queries", () => ({
    useCreateSalesOrderMutation: () => ({
        mutateAsync: mutations.create,
        isPending: false,
    }),
    useSaveSalesOrderDraftMutation: () => ({
        mutateAsync: mutations.save,
        isPending: false,
    }),
    useSubmitSalesOrderMutation: () => ({
        mutateAsync: mutations.submit,
        isPending: false,
    }),
}))
beforeEach(() => {
    vi.clearAllMocks()
    mutations.create.mockResolvedValue({
        salesOrderId: "order-1",
        documentNumber: "SO-001",
        workingCopyVersion: 1,
    })
    mutations.save.mockResolvedValue({ version: 2 })
})
afterEach(cleanup)

function harness() {
    const commandLedger = new FormalCommandKeyLedger()
    return renderHook(() => ({
        values: useSalesOrderCreateDefaults({
            initialCustomerId: "",
            initialContractId: "contract-1",
            initialContractRevisionId: "revision-1",
            initialNature: "physical_service",
            initialDraft: null,
        }),
        submission: useSalesOrderCreateSubmission({
            initialDraft: null,
            commandLedger,
        }),
    }))
}

test("new and existing draft saves retain the exact saved input without resetting ongoing edits", async () => {
    const { result } = harness()
    const form = { reset: vi.fn() }
    const first = { ...result.current.values, remark: "第一次保存" }
    await act(async () => result.current.submission.handleSubmit(first, form))
    expect(result.current.submission.savedValues).toEqual(first)
    const second = { ...first, remark: "第二次保存" }
    await act(async () => result.current.submission.handleSubmit(second, form))
    expect(result.current.submission.savedValues).toEqual(second)
    expect(mutations.create).toHaveBeenCalledTimes(1)
    expect(mutations.save).toHaveBeenCalledTimes(1)
    expect(form.reset).not.toHaveBeenCalled()
})

test("retrying a failed creation does not mark different current input as saved", async () => {
    const { result } = harness()
    const form = { reset: vi.fn() }
    const first = { ...result.current.values, remark: "实际重试的内容" }
    mutations.create.mockRejectedValueOnce(
        Object.assign(new Error("网络中断"), { kind: "Network" }),
    )
    await act(async () => {
        await expect(
            result.current.submission.handleSubmit(first, form),
        ).rejects.toThrow("网络中断")
    })
    const edited = { ...first, remark: "尚未保存的新输入" }
    await act(async () => result.current.submission.handleSubmit(edited, form))
    expect(mutations.create.mock.lastCall?.[0].remark).toBe(first.remark)
    expect(result.current.submission.savedValues).toBeNull()
    expect(form.reset).not.toHaveBeenCalled()
})
