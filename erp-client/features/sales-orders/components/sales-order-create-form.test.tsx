import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { useAppForm } from "@/components/form"
import { revalidateLogic } from "@tanstack/react-form"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders-create"
import { validateSalesOrderForm } from "@/features/sales-orders/lib/sales-order-create-validation"
import type { SalesOrderCreateFormValues } from "@/features/sales-orders/lib/sales-order-create-validation"

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn(), prefetch: vi.fn() }),
}))

vi.mock("@/features/contracts/contract-upload-dialog", () => ({
    ContractUploadDialog: () => null,
}))

vi.mock("@/features/entity-selectors", () => ({
    entitySelectorKeys: { all: ["entity-selectors"] as const },
    ContractSearchCombobox: () => null,
    MallSearchCombobox: () => null,
    SellableSkuSearchCombobox: ({
        onValueChange,
        onItemChange,
    }: {
        onValueChange?: (id?: string) => void
        onItemChange?: (item?: {
            revisionId: string
            name: string
            baseUnit: string
        }) => void
    }) => (
        <button
            type="button"
            onClick={() => {
                onValueChange?.("sku-1")
                onItemChange?.({
                    revisionId: "sr-1",
                    name: "货物",
                    baseUnit: "件",
                })
            }}
        >
            选择商品
        </button>
    ),
    VoucherCategorySearchCombobox: () => null,
}))

vi.mock("@/features/contracts/queries", () => ({
    useContractCenterQuery: () => ({
        data: {
            contractId: "ct-1",
            contractNo: "HT-2026-001",
            customer: { id: "cu-1", displayName: "客户甲" },
            currentRevision: {
                revisionId: "r-1",
                revisionNo: 1,
                settlementParty: { id: "sp-1", displayName: "结算主体甲" },
                paymentTermSnapshot: { label: "POSTPAY_NET30" },
            },
            revisionTimeline: [
                { revisionId: "r-1", revisionNo: 1, isCurrent: true },
            ],
        },
        isFetching: false,
        isPending: false,
        isError: false,
        error: null,
    }),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { userid: "u-1", name: "张三", account: "zhangsan" },
        isPending: false,
        isError: false,
        error: null,
    }),
}))

vi.mock("@/components/ui/date-picker", () => ({
    DatePicker: ({
        value,
        onValueChange,
        placeholder = "选择日期",
    }: {
        value?: string
        onValueChange?: (value?: string) => void
        placeholder?: string
    }) => (
        <button
            type="button"
            aria-label={value ? `已选日期 ${value}` : placeholder}
            onClick={() => onValueChange?.(value ? undefined : "2026-09-01")}
        >
            {value ?? placeholder}
        </button>
    ),
    DateTimeLocalPicker: () => null,
}))

const filledLine = {
    rowKey: "l1",
    name: "货物",
    sku: "sku-1",
    skuRevisionId: "sr-1",
    quantity: "1",
    unit: "件",
    unitPriceGross: "0.00",
    fulfillmentMode: "公司仓发",
    dueDate: "",
    faceValue: "",
    giftRate: "",
    cardForm: "",
} as const

const incompleteLine = {
    ...filledLine,
    name: "",
    sku: "",
    skuRevisionId: "",
    unit: "",
}

function makeDraft(
    line: typeof filledLine | typeof incompleteLine,
): SalesOrderDraftResumeData {
    return {
        salesOrderId: "so-1",
        documentNumber: "SO-2026-001",
        version: 1,
        contractId: "ct-1",
        nature: "physical_service",
        welfareScene: "ANNUAL_GIFT_BAG",
        paymentTerms: "POSTPAY_NET30",
        fulfillmentDeadline: "2026-09-30",
        targetMallId: "",
        receivableDueDate: "",
        taxRatePercent: "13.00",
        remark: "",
        lineItems: [{ ...line }],
    }
}

const completeHeader = {
    contractId: "ct-1",
    requestedContractRevisionId: "r-1",
    contractRevisionLabel: "CT-1@v1",
    customerId: "cu-1",
    customerName: "客户甲",
    settlementPartyId: "sp-1",
    settlementEntity: "结算主体甲",
    nature: "physical_service" as const,
    ownerUserId: "u-1",
    ownerName: "张三",
    welfareScene: "ANNUAL_GIFT_BAG",
    paymentTerms: "POSTPAY_NET30",
    fulfillmentDeadline: "2026-09-30",
    targetMallId: "",
    receivableDueDate: "",
    taxRatePercent: "13.00",
    remark: "",
}

let queryClient: QueryClient

function renderCreateForm(draft: SalesOrderDraftResumeData) {
    queryClient = new QueryClient({
        defaultOptions: {
            queries: { retry: false },
            mutations: { retry: false },
        },
    })
    return render(
        <QueryClientProvider client={queryClient}>
            <SalesOrderCreateForm initialDraft={draft} purpose="create" />
        </QueryClientProvider>,
    )
}

afterEach(() => {
    cleanup()
    queryClient?.clear()
})

describe("SalesOrderCreateForm submit recovery", () => {
    it("re-enables submit after fixing unit price and due date", async () => {
        renderCreateForm(makeDraft(filledLine))
        const button = screen.getByRole("button", {
            name: "提交",
        }) as HTMLButtonElement

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        })

        fireEvent.click(button)
        await waitFor(() => {
            expect(button.disabled).toBe(true)
        })
        expect(screen.getByText("含税单价必须大于 0")).toBeTruthy()
        expect(screen.getByText("请选择明细交付日期")).toBeTruthy()

        fireEvent.change(screen.getByLabelText("含税单价"), {
            target: { value: "100.00" },
        })
        await waitFor(() => {
            expect(screen.queryByText("含税单价必须大于 0")).toBeNull()
        })

        fireEvent.click(screen.getByRole("button", { name: "选择日期" }))
        await waitFor(() => {
            expect(screen.queryByText("请选择明细交付日期")).toBeNull()
        })

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        })
    })

    it("re-enables submit after selecting SKU then filling price and date", async () => {
        renderCreateForm(makeDraft(incompleteLine))
        const button = screen.getByRole("button", {
            name: "提交",
        }) as HTMLButtonElement

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        })

        fireEvent.click(button)
        await waitFor(() => {
            expect(button.disabled).toBe(true)
        })
        expect(screen.getByText("含税单价必须大于 0")).toBeTruthy()
        expect(screen.getByText("请选择明细交付日期")).toBeTruthy()

        fireEvent.click(screen.getByRole("button", { name: "选择商品" }))
        fireEvent.change(screen.getByLabelText("含税单价"), {
            target: { value: "100.00" },
        })
        fireEvent.click(screen.getByRole("button", { name: "选择日期" }))

        await waitFor(() => {
            expect(screen.queryByText("含税单价必须大于 0")).toBeNull()
            expect(screen.queryByText("请选择明细交付日期")).toBeNull()
            expect(button.disabled).toBe(false)
        })
    })
})

function IsolatedRecoveryForm() {
    const form = useAppForm({
        defaultValues: {
            ...completeHeader,
            lineItems: [{ ...incompleteLine }],
        } satisfies SalesOrderCreateFormValues,
        validationLogic: revalidateLogic(),
        validators: {
            onDynamic: ({ value }) => validateSalesOrderForm(value, "SUBMIT"),
        },
        onSubmit: () => undefined,
    })

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField name="lineItems[0].unitPriceGross">
                {(field) => (
                    <input
                        aria-label="含税单价"
                        value={field.state.value}
                        onChange={(event) =>
                            field.handleChange(event.target.value)
                        }
                    />
                )}
            </form.AppField>
            <form.AppField name="lineItems[0].dueDate">
                {(field) => (
                    <input
                        aria-label="交付日期"
                        value={field.state.value}
                        onChange={(event) =>
                            field.handleChange(event.target.value)
                        }
                    />
                )}
            </form.AppField>
            <button
                type="button"
                onClick={() => {
                    form.setFieldValue("lineItems[0].sku", "sku-1")
                    form.setFieldValue("lineItems[0].skuRevisionId", "sr-1")
                    form.setFieldValue("lineItems[0].name", "货物")
                    form.setFieldValue("lineItems[0].unit", "件")
                }}
            >
                补全商品
            </button>
            <form.AppForm>
                <form.SubmitButton label="提交" />
            </form.AppForm>
        </form>
    )
}

describe("submit button recovers when name is only set via setFieldValue", () => {
    it("re-enables after filling unmounted name plus price and date", async () => {
        render(<IsolatedRecoveryForm />)
        const button = screen.getByRole("button", {
            name: "提交",
        }) as HTMLButtonElement
        expect(button.disabled).toBe(false)

        fireEvent.click(button)
        await waitFor(() => {
            expect(button.disabled).toBe(true)
        })

        fireEvent.click(screen.getByRole("button", { name: "补全商品" }))
        fireEvent.change(screen.getByLabelText("含税单价"), {
            target: { value: "100.00" },
        })
        fireEvent.change(screen.getByLabelText("交付日期"), {
            target: { value: "2026-09-01" },
        })

        await waitFor(() => {
            expect(button.disabled).toBe(false)
        })
    })
})
