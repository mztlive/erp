import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import { RegisterSupplyForSkuDialog } from "./register-supply-for-sku-dialog"

const mutation = vi.hoisted(() => ({ isPending: false, mutateAsync: vi.fn() }))
vi.mock("@/features/supplier-offerings/hooks/queries", () => ({
    useCreateSupplierOfferingMutation: () => mutation,
}))
vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(async () => ({
        current_profile: { invoice_tax_rates: ["0.13"] },
    })),
}))
vi.mock("@/components/ui/toast", () => ({ toast: { add: vi.fn() } }))
vi.mock("@/features/entity-selectors", () => ({
    SupplierSearchCombobox: ({
        id,
        value,
        onValueChange,
        onBlur,
    }: {
        id: string
        value: string
        onValueChange: (v: string) => void
        onBlur: () => void
    }) => (
        <input
            id={id}
            aria-label="供应商"
            value={value ?? ""}
            onChange={(e) => onValueChange(e.target.value)}
            onBlur={onBlur}
        />
    ),
    CompanySkuSearchCombobox: () => null,
}))

beforeEach(() => {
    mutation.isPending = false
    mutation.mutateAsync.mockReset().mockResolvedValue({ id: "offering-1" })
    Element.prototype.scrollIntoView = vi.fn()
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

function mount(onOpenChange = vi.fn()) {
    render(
        <QueryClientProvider
            client={
                new QueryClient({
                    defaultOptions: { queries: { retry: false } },
                })
            }
        >
            <RegisterSupplyForSkuDialog
                open
                onOpenChange={onOpenChange}
                fixedSku={{
                    skuId: "sku-1",
                    skuCode: "SKU-1",
                    skuName: "测试商品",
                    specification: "",
                    baseUnit: "件",
                }}
            />
        </QueryClientProvider>,
    )
    return onOpenChange
}

async function fillRequired() {
    fireEvent.change(screen.getByLabelText("供应商"), {
        target: { value: "supplier-1" },
    })
    await waitFor(() =>
        expect(
            (screen.getByLabelText(/进项税率/) as HTMLInputElement).value,
        ).toBe("13"),
    )
    fireEvent.change(screen.getByLabelText(/供应商订货编码/), {
        target: { value: "SUP-1" },
    })
    fireEvent.change(screen.getByLabelText(/一件代发价/), {
        target: { value: "12.3456" },
    })
    fireEvent.click(screen.getByRole("button", { name: "代发价填入集采价" }))
    const region = screen.getByRole("combobox", { name: "可供区域" })
    fireEvent.focus(region)
    fireEvent.change(region, { target: { value: "上海浦东" } })
    fireEvent.keyDown(region, { key: "ArrowDown" })
    fireEvent.click(
        await screen.findByRole("option", { name: "添加“上海浦东”" }),
    )
    fireEvent.blur(region)
}

test("默认关闭不提示放弃；编辑后可继续填写或确认放弃", async () => {
    const close = mount()
    expect(screen.getByText("选择供应商后显示常用税率。")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "取消" }))
    expect(close).toHaveBeenCalledWith(false)
    close.mockClear()
    fireEvent.change(screen.getByLabelText(/供应商订货编码/), {
        target: { value: "draft" },
    })
    fireEvent.click(screen.getByRole("button", { name: "关闭" }))
    expect(await screen.findByRole("alertdialog")).toBeTruthy()
    expect(close).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "继续编辑" }))
    expect(
        (screen.getByLabelText(/供应商订货编码/) as HTMLInputElement).value,
    ).toBe("draft")
    fireEvent.click(screen.getByRole("button", { name: "取消" }))
    fireEvent.click(await screen.findByRole("button", { name: "放弃更改" }))
    expect(close).toHaveBeenCalledWith(false)
})

test("复制含税价格并保留自定义区域，长期有效提交空失效日期，成功关闭", async () => {
    const close = mount()
    await fillRequired()
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    await waitFor(() => {
        expect(
            screen.queryAllByRole("alert").map((el) => el.textContent),
        ).toEqual([])
        expect(mutation.mutateAsync).toHaveBeenCalledTimes(1)
    })
    expect(mutation.mutateAsync.mock.calls[0][0]).toMatchObject({
        sku_id: "sku-1",
        supplier_id: "supplier-1",
        supplier_sku_code: "SUP-1",
        source_type: "MANUAL",
        terms: {
            dropship_supply_price_gross: "12.3456",
            bulk_supply_price_gross: "12.3456",
            input_tax_rate: "0.130000",
            bulk_minimum_order_quantity: "1",
            supply_region: ["上海浦东"],
            valid_to: null,
        },
        availability_status: "AVAILABLE",
        available_quantity: null,
        change_reason: "新增供应商供给",
    })
    expect(close).toHaveBeenCalledWith(false)
})

test("保存失败保留录入内容，允许修正后重试", async () => {
    mutation.mutateAsync.mockRejectedValueOnce(new Error("暂时无法保存"))
    const close = mount()
    await fillRequired()
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    expect(await screen.findByText("保存失败")).toBeTruthy()
    expect(close).not.toHaveBeenCalled()
    expect(
        (screen.getByLabelText(/供应商订货编码/) as HTMLInputElement).value,
    ).toBe("SUP-1")
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    await waitFor(() => expect(mutation.mutateAsync).toHaveBeenCalledTimes(2))
    expect(close).toHaveBeenCalledWith(false)
})

test("请求未完成时关闭按钮和 Escape 都不能丢弃表单", async () => {
    mutation.isPending = true
    const close = mount()
    fireEvent.click(screen.getByRole("button", { name: "关闭" }))
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })
    expect(close).not.toHaveBeenCalled()
    expect(
        (screen.getByRole("button", { name: "正在保存…" }) as HTMLButtonElement)
            .disabled,
    ).toBe(true)
})

test("指定失效日期必填；切回长期有效后移除错误并可保存", async () => {
    const close = mount()
    await fillRequired()
    fireEvent.keyDown(screen.getByRole("combobox", { name: "有效期" }), {
        key: "ArrowDown",
    })
    fireEvent.click(await screen.findByRole("option", { name: "指定失效日期" }))
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    expect(await screen.findByText("请选择失效日期")).toBeTruthy()
    expect(mutation.mutateAsync).not.toHaveBeenCalled()
    fireEvent.keyDown(screen.getByRole("combobox", { name: "有效期" }), {
        key: "ArrowDown",
    })
    fireEvent.click(await screen.findByRole("option", { name: "长期有效" }))
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    await waitFor(() => expect(close).toHaveBeenCalledWith(false))
    expect(mutation.mutateAsync.mock.calls[0][0].terms.valid_to).toBeNull()
})

test("折叠补充信息中的错误会展开并聚焦到输入框", async () => {
    mount()
    await fillRequired()
    fireEvent.click(screen.getByText("补充信息"))
    fireEvent.change(screen.getByLabelText("运费"), { target: { value: "-1" } })
    const details = screen.getByText("补充信息").closest("details")!
    details.open = false
    fireEvent(details, new Event("toggle"))
    fireEvent.click(screen.getByRole("button", { name: "保存供给" }))
    await waitFor(() => expect(details.open).toBe(true))
    await waitFor(() =>
        expect(document.activeElement?.id).toBe(
            "supplier-offerings-dialog-register-freight-amount",
        ),
    )
    expect(mutation.mutateAsync).not.toHaveBeenCalled()
})
