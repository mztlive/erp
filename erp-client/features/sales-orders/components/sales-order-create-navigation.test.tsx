import {
    act,
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import { SalesOrderCreateForm } from "./sales-order-create-form"

const mocks = vi.hoisted(() => ({
    push: vi.fn(),
    form: null as SalesOrderCreateFormApi | null,
    savedValues: null as CreateSalesOrderFormValues | null,
}))
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: mocks.push }) }))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { userid: "sales-1", name: "销售甲" },
        isPending: false,
        isError: false,
    }),
}))
vi.mock("@/features/contracts/queries", () => ({
    useContractCenterQuery: () => ({ data: undefined, isFetching: false }),
}))
vi.mock("@/features/contracts/contract-upload-dialog", () => ({
    ContractUploadDialog: () => null,
}))
vi.mock(
    "@/features/sales-orders/hooks/use-sales-line-procurement-responsibilities",
    () => ({
        useSalesLineProcurementResponsibilities: () => ({
            allResolved: true,
            byRowKey: new Map(),
            isFetching: false,
            isError: false,
        }),
    }),
)
vi.mock(
    "@/features/sales-orders/hooks/use-sales-order-create-submission",
    () => ({
        useSalesOrderCreateSubmission: () => ({
            submitIntentRef: { current: "SAVE_DRAFT" },
            setDraftSaved: vi.fn(),
            savedValues: mocks.savedValues,
            createMutation: {},
            isSubmitting: false,
        }),
    }),
)
vi.mock("./sales-order-create-alerts", () => ({
    SalesOrderCreateAlerts: () => null,
}))
vi.mock("./sales-order-create-contract-section", () => ({
    SalesOrderCreateContractSection: () => null,
}))
vi.mock("./sales-order-create-line-items-section", () => ({
    SalesOrderCreateLineItemsSection: () => null,
}))
vi.mock("./sales-order-create-total-bar", () => ({
    SalesOrderCreateTotalBar: () => null,
}))
vi.mock("./sales-order-submit-confirm-dialog", () => ({
    SalesOrderSubmitConfirmDialog: () => null,
}))
vi.mock("./voucher-sales-order-submit-confirm-dialog", () => ({
    VoucherSalesOrderSubmitConfirmDialog: () => null,
}))
vi.mock("./sales-order-create-header-fields", () => ({
    SalesOrderCreateHeaderFields: ({
        form,
    }: {
        form: SalesOrderCreateFormApi
    }) => {
        mocks.form = form
        return (
            <form.AppField name="remark">
                {(field) => <field.TextField label="内部说明" />}
            </form.AppField>
        )
    },
}))

beforeEach(() => {
    mocks.push.mockReset()
    mocks.savedValues = null
    mocks.form = null
})
afterEach(cleanup)

function mount() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    const tree = () => (
        <QueryClientProvider client={client}>
            <SalesOrderCreateForm />
        </QueryClientProvider>
    )
    const view = render(tree())
    return { ...view, refresh: () => view.rerender(tree()) }
}

test("uses the shared secondary header and automatic owner hydration does not block return", async () => {
    mount()
    await waitFor(() =>
        expect(mocks.form?.state.values.ownerName).toBe("销售甲"),
    )
    expect(
        screen
            .getByRole("heading", { name: "新建销售单" })
            .closest('[data-slot="detail-page-header"]'),
    ).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "销售单列表" }))
    expect(mocks.push).toHaveBeenCalledWith("/sales/orders")
    expect(screen.queryByRole("alertdialog")).toBeNull()
})

test("cancel preserves unsaved input; confirming return navigates to the list", async () => {
    mount()
    fireEvent.change(screen.getByLabelText("内部说明"), {
        target: { value: "保留本次输入" },
    })
    fireEvent.click(screen.getByRole("button", { name: "销售单列表" }))
    expect(screen.getByRole("alertdialog")).toBeTruthy()
    expect(mocks.push).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "继续编辑" }))
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull())
    expect((screen.getByLabelText("内部说明") as HTMLInputElement).value).toBe(
        "保留本次输入",
    )
    fireEvent.click(screen.getByRole("button", { name: "销售单列表" }))
    fireEvent.click(screen.getByRole("button", { name: "放弃更改并返回" }))
    expect(mocks.push).toHaveBeenCalledWith("/sales/orders")
})

test("saved input may return directly, while edits made during saving remain protected", async () => {
    const view = mount()
    fireEvent.change(screen.getByLabelText("内部说明"), {
        target: { value: "已保存内容" },
    })
    mocks.savedValues = structuredClone(mocks.form!.state.values)
    await act(async () => view.refresh())
    fireEvent.click(screen.getByRole("button", { name: "销售单列表" }))
    expect(mocks.push).toHaveBeenCalledWith("/sales/orders")
    mocks.push.mockClear()
    fireEvent.change(screen.getByLabelText("内部说明"), {
        target: { value: "保存期间继续修改" },
    })
    fireEvent.click(screen.getByRole("button", { name: "销售单列表" }))
    expect(screen.getByRole("alertdialog")).toBeTruthy()
    expect(mocks.push).not.toHaveBeenCalled()
})
