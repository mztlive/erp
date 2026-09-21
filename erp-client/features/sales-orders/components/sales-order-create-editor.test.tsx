import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { revalidateLogic } from "@tanstack/react-form"
import { validateSalesOrderForm } from "@/features/sales-orders/lib/sales-order-create-model"
import { useAppForm } from "@/components/form"
import { useSalesOrderCreateDefaults } from "@/features/sales-orders/hooks/use-sales-order-create-defaults"
import { SalesOrderCreateLineItemsSection } from "./sales-order-create-line-items-section"
import { SalesOrderCreateTotalBar } from "./sales-order-create-total-bar"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import type { SalesOrderNature } from "@/features/sales-orders/types"

const tea: SellableSkuPick = {
    skuId: "tea",
    skuRevisionId: "tea-v1",
    skuNo: "TEA-001",
    name: "茶礼盒",
    specificationLabel: "100g × 2 盒",
    baseUnit: "盒",
    salesVisiblePriceGross: "113.00",
}
const cup: SellableSkuPick = {
    ...tea,
    skuId: "cup",
    skuRevisionId: "cup-v1",
    name: "保温杯",
    specificationLabel: "500ml",
}

vi.mock("./sellable-sku-select-dialog", () => ({
    SellableSkuSelectDialog: ({
        open,
        multiple,
        onConfirm,
        onOpenChange,
    }: {
        open: boolean
        multiple: boolean
        onConfirm: (picks: SellableSkuPick[]) => void
        onOpenChange: (open: boolean) => void
    }) =>
        open ? (
            <button
                type="button"
                onClick={() => {
                    onConfirm(multiple ? [tea, cup] : [cup])
                    onOpenChange(false)
                }}
            >
                确认选品
            </button>
        ) : null,
}))
vi.mock("./voucher-category-search-combobox", () => ({
    VoucherCategorySearchCombobox: () => (
        <button type="button">搜索卡券类目</button>
    ),
}))

afterEach(cleanup)

function Harness({
    nature = "physical_service",
    fetching = false,
    error = false,
    remark = "",
}: {
    nature?: SalesOrderNature
    fetching?: boolean
    error?: boolean
    remark?: string
}) {
    const defaultValues = useSalesOrderCreateDefaults({
        initialCustomerId: "",
        initialContractId: "",
        initialContractRevisionId: "",
        initialNature: nature,
        initialDraft: null,
    })
    const form = useAppForm({ defaultValues: { ...defaultValues, remark } })
    return (
        <form>
            <SalesOrderCreateLineItemsSection
                form={form}
                procurementFetching={fetching}
                procurementError={error}
            />
            <SalesOrderCreateTotalBar
                form={form}
                isSubmitting={false}
                onSaveDraftClick={() => {}}
                onSubmitClick={() => {}}
            />
        </form>
    )
}

test("physical orders start empty; add and replace preserve quantity, and the last line can be removed", async () => {
    render(<Harness />)
    expect(screen.getByText("尚未添加商品")).toBeTruthy()
    expect(screen.queryByRole("table")).toBeNull()
    expect(screen.queryByText(/暂不能提交/)).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "添加商品" }))
    fireEvent.click(screen.getByRole("button", { name: "确认选品" }))
    await waitFor(() =>
        expect(screen.getAllByLabelText("数量")).toHaveLength(2),
    )
    expect(screen.getByText("100g × 2 盒")).toBeTruthy()
    expect(screen.getByText(/可以保存草稿/)).toBeTruthy()
    fireEvent.change(screen.getAllByLabelText("数量")[0]!, {
        target: { value: "3" },
    })
    await waitFor(() => expect(screen.getByText("¥452.00")).toBeTruthy())
    fireEvent.click(screen.getByRole("button", { name: "更换销售项目 茶礼盒" }))
    fireEvent.click(screen.getByRole("button", { name: "确认选品" }))
    await waitFor(() =>
        expect(
            screen.queryByRole("button", { name: "更换销售项目 茶礼盒" }),
        ).toBeNull(),
    )
    expect(
        (screen.getAllByLabelText("数量")[0] as HTMLInputElement).value,
    ).toBe("3")
    expect(screen.getAllByLabelText("数量")).toHaveLength(2)
    const removeButtons = () => screen.getAllByRole("button", { name: /删除/ })
    fireEvent.click(removeButtons()[0]!)
    await waitFor(() =>
        expect(screen.getAllByLabelText("数量")).toHaveLength(1),
    )
    fireEvent.click(removeButtons()[0]!)
    await waitFor(() => expect(screen.getByText("尚未添加商品")).toBeTruthy())
    expect(screen.getAllByRole("button", { name: "添加商品" })).toHaveLength(1)
})

test("voucher orders keep their single category line and do not expose product addition", () => {
    render(<Harness nature="card_voucher" />)
    expect(screen.getByRole("button", { name: "搜索卡券类目" })).toBeTruthy()
    expect(screen.getAllByLabelText("数量")).toHaveLength(1)
    expect(screen.queryByRole("button", { name: "添加商品" })).toBeNull()
    expect(
        screen.getByRole("button", { name: /删除/ }).hasAttribute("disabled"),
    ).toBe(true)
})

test("procurement loading does not show configuration failure; request failure has its own message", async () => {
    const view = render(<Harness fetching />)
    fireEvent.click(screen.getByRole("button", { name: "添加商品" }))
    fireEvent.click(screen.getByRole("button", { name: "确认选品" }))
    await waitFor(() =>
        expect(screen.getAllByText("正在匹配…")).toHaveLength(2),
    )
    expect(screen.queryByText(/请联系管理员/)).toBeNull()
    view.rerender(<Harness error />)
    await waitFor(() =>
        expect(screen.getAllByText("匹配失败，暂不能提交")).toHaveLength(2),
    )
    expect(screen.getByText(/采购负责人暂时无法匹配，请稍后重试/)).toBeTruthy()
})

test("an empty order shows the line error and can recover after adding a product", async () => {
    const save = vi.fn()
    function ValidatedHarness() {
        const defaults = useSalesOrderCreateDefaults({
            initialCustomerId: "",
            initialContractId: "contract",
            initialContractRevisionId: "",
            initialNature: "physical_service",
            initialDraft: null,
        })
        const form = useAppForm({
            defaultValues: defaults,
            validationLogic: revalidateLogic(),
            validators: {
                onDynamic: ({ value }) =>
                    validateSalesOrderForm(value, "SAVE_DRAFT"),
            },
            onSubmit: save,
        })
        return (
            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <SalesOrderCreateLineItemsSection form={form} />
                <form.AppForm>
                    <form.SubmitButton label="保存草稿" />
                </form.AppForm>
            </form>
        )
    }
    render(<ValidatedHarness />)
    fireEvent.click(screen.getByRole("button", { name: "保存草稿" }))
    await waitFor(() =>
        expect(screen.getByText("至少需要一条销售明细")).toBeTruthy(),
    )
    expect(save).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "添加商品" }))
    fireEvent.click(screen.getByRole("button", { name: "确认选品" }))
    await waitFor(() =>
        expect(
            screen
                .getByRole("button", { name: "保存草稿" })
                .hasAttribute("disabled"),
        ).toBe(false),
    )
    fireEvent.click(screen.getByRole("button", { name: "保存草稿" }))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByRole("button", { name: "删除第 1 行" }))
    await waitFor(() =>
        expect(screen.getAllByLabelText("数量")).toHaveLength(1),
    )
    fireEvent.click(screen.getByRole("button", { name: "删除第 1 行" }))
    await waitFor(() =>
        expect(screen.getByText("至少需要一条销售明细")).toBeTruthy(),
    )
    expect(save).toHaveBeenCalledTimes(1)
})

test("collapsing internal notes preserves their value when reopened", async () => {
    render(<Harness />)
    const toggle = screen.getByRole("button", { name: "内部说明（选填）" })
    expect(toggle.getAttribute("aria-expanded")).toBe("false")
    fireEvent.click(toggle)
    fireEvent.change(
        screen.getByRole("textbox", { name: "内部说明（选填）" }),
        {
            target: { value: "交付前联系客户" },
        },
    )
    const filledToggle = screen.getByRole("button", {
        name: "内部说明（已填写）",
    })
    fireEvent.click(filledToggle)
    await waitFor(() =>
        expect(filledToggle.getAttribute("aria-expanded")).toBe("false"),
    )
    fireEvent.click(filledToggle)
    expect(
        (
            screen.getByRole("textbox", {
                name: "内部说明（选填）",
            }) as HTMLTextAreaElement
        ).value,
    ).toBe("交付前联系客户")
})

test("existing internal notes are expanded when the editor opens", () => {
    render(<Harness remark="客户已确认交付安排" />)
    expect(
        screen
            .getByRole("button", { name: "内部说明（已填写）" })
            .getAttribute("aria-expanded"),
    ).toBe("true")
    expect(
        (
            screen.getByRole("textbox", {
                name: "内部说明（选填）",
            }) as HTMLTextAreaElement
        ).value,
    ).toBe("客户已确认交付安排")
})
