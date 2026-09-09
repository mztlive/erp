import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { InvoiceRequestForm } from "./request-form"
import type { SubmitRequest } from "../api"

const submit = vi.hoisted(() => vi.fn())
vi.mock("../hooks/queries", () => ({
    useInvoiceRequestAmounts: () => ({ data: { available_amount: "1000.00" } }),
    useInvoiceRequestCommands: () => ({
        submit: { isPending: false, mutateAsync: submit },
    }),
}))
vi.mock(
    "@/features/entity-selectors/components/sales-order-search-combobox",
    () => ({ SalesOrderSearchCombobox: () => null }),
)
afterEach(cleanup)
beforeEach(() => submit.mockReset())
function setup() {
    const onDone = vi.fn()
    const onCancel = vi.fn()
    render(
        <QueryClientProvider
            client={
                new QueryClient({
                    defaultOptions: { queries: { retry: false } },
                })
            }
        >
            <InvoiceRequestForm
                salesOrderId="sale-1"
                accountId="account-1"
                title="测试客户"
                onDone={onDone}
                onCancel={onCancel}
            />
        </QueryClientProvider>,
    )
    for (const [id, value] of [
        ["amount", "600.00"],
        ["tax-number", "TAX-1"],
        ["content", "服务费"],
        ["reason", "按合同约定申请"],
    ]) {
        fireEvent.change(document.getElementById(`invoice-request-${id}`)!, {
            target: { value },
        })
    }
    return { onDone, onCancel }
}
it("提交固定销售应收及开票要求，申请通过前没有开票动作", async () => {
    submit.mockImplementation(async (input: SubmitRequest) => ({
        id: "request-1",
        ...input,
    }))
    const { onDone } = setup()
    fireEvent.click(screen.getByRole("button", { name: "提交审批" }))
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1))
    expect(submit.mock.calls[0][0]).toMatchObject({
        receivable_account_id: "account-1",
        data: {
            amount: "600.00",
            invoice_title: "测试客户",
            tax_number: "TAX-1",
            invoice_content: "服务费",
            reason: "按合同约定申请",
        },
    })
    expect(submit.mock.calls[0][0].idempotency_key).toBeTruthy()
    expect(screen.queryByRole("button", { name: "登记发票" })).toBeNull()
})
it("金额不精确到分时不提交", async () => {
    setup()
    fireEvent.change(document.getElementById("invoice-request-amount")!, {
        target: { value: "1.001" },
    })
    fireEvent.click(screen.getByRole("button", { name: "提交审批" }))
    await screen.findByText("金额最多两位小数")
    expect(submit).not.toHaveBeenCalled()
})
it("提交结果未知时锁定编辑和取消，并保留相同命令与载荷核对", async () => {
    submit
        .mockRejectedValueOnce({ kind: "Network", message: "连接中断" })
        .mockResolvedValueOnce({ id: "request-1" })
    const { onDone, onCancel } = setup()
    fireEvent.click(screen.getByRole("button", { name: "提交审批" }))
    await screen.findByRole("button", { name: "核对提交结果" })
    expect(document.querySelector("fieldset")?.disabled).toBe(true)
    expect(
        (screen.getByRole("button", { name: "取消" }) as HTMLButtonElement)
            .disabled,
    ).toBe(true)
    fireEvent.click(screen.getByRole("button", { name: "取消" }))
    expect(onCancel).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "核对提交结果" }))
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1))
    expect(submit.mock.calls[1][0]).toEqual(submit.mock.calls[0][0])
})
