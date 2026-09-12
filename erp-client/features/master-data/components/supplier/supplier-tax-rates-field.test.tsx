import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { useAppForm } from "@/components/form"
import { afterEach, expect, test, vi } from "vitest"
import { SupplierTaxRatesField } from "./supplier-tax-rates-field"

afterEach(cleanup)

function mount(value = "9、13", disabled = false) {
    const onSubmit = vi.fn()
    function Harness() {
        const form = useAppForm({ defaultValues: { rates: value } })
        return (
            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    onSubmit()
                }}
            >
                <form.AppField name="rates">
                    {(field) => (
                        <>
                            <SupplierTaxRatesField
                                id="supplier-tax"
                                value={field.state.value}
                                onChange={field.handleChange}
                                disabled={disabled}
                            />
                            <output data-testid="value">
                                {field.state.value}
                            </output>
                        </>
                    )}
                </form.AppField>
            </form>
        )
    }
    render(<Harness />)
    return onSubmit
}

async function openInput() {
    fireEvent.click(screen.getByRole("button", { name: "添加税率" }))
    return screen.findByRole("textbox", { name: "税率（%）" })
}

test("已有多税率逐项展示，添加后排序并写回原表单，回车不提交供应商", async () => {
    const onSubmit = mount()
    expect(screen.getByText("9%")).toBeTruthy()
    expect(screen.getByText("13%")).toBeTruthy()
    const input = await openInput()
    fireEvent.change(input, { target: { value: "6%" } })
    fireEvent.keyDown(input, { key: "Enter" })
    await waitFor(() =>
        expect(screen.getByTestId("value").textContent).toBe("6、9、13"),
    )
    expect(onSubmit).not.toHaveBeenCalled()
})

test("空白、重复、非法和多项输入不能添加，取消保留原值", async () => {
    mount()
    const input = await openInput()
    for (const value of ["", "9", "abc", "100", "6、13"]) {
        fireEvent.change(input, { target: { value } })
        fireEvent.click(screen.getByRole("button", { name: "添加" }))
        await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy())
        expect(screen.getByTestId("value").textContent).toBe("9、13")
    }
    fireEvent.change(input, { target: { value: "6" } })
    fireEvent.click(screen.getByRole("button", { name: "取消" }))
    expect(screen.getByTestId("value").textContent).toBe("9、13")
    const resetInput = await openInput()
    expect((resetInput as HTMLInputElement).value).toBe("")
})

test("删除全部表示未登记，零税率可明确添加", async () => {
    mount()
    fireEvent.click(screen.getByRole("button", { name: "删除税率 9%" }))
    fireEvent.click(screen.getByRole("button", { name: "删除税率 13%" }))
    expect(screen.getByTestId("value").textContent).toBe("")
    expect(screen.getByText("未登记")).toBeTruthy()
    const input = await openInput()
    fireEvent.change(input, { target: { value: "0" } })
    fireEvent.click(screen.getByRole("button", { name: "添加" }))
    await waitFor(() =>
        expect(screen.getByTestId("value").textContent).toBe("0"),
    )
    expect(screen.getByText("0%")).toBeTruthy()
})

test("无编辑权限时不能添加或删除", () => {
    mount("9、13", true)
    for (const button of screen.getAllByRole("button")) {
        expect((button as HTMLButtonElement).disabled).toBe(true)
        fireEvent.click(button)
    }
    expect(screen.getByTestId("value").textContent).toBe("9、13")
    expect(screen.queryByRole("textbox")).toBeNull()
})
