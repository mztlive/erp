import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"
import { useAppForm } from "@/components/form"
import { createSpecDraft } from "../../lib/product-editor-model"
import { ProductSpecDraftsEditor } from "./product-spec-drafts"

afterEach(cleanup)

function Harness() {
    const form = useAppForm({
        defaultValues: {
            drafts: [
                createSpecDraft("颜色", ["红色"]),
                createSpecDraft("尺寸", ["大号"]),
            ],
        },
    })
    return (
        <form.Subscribe selector={(state) => state.values.drafts}>
            {(drafts) => (
                <ProductSpecDraftsEditor
                    canRevise
                    specDrafts={drafts}
                    skuCount={1}
                    syncSpecDrafts={(next) =>
                        form.setFieldValue("drafts", [...next])
                    }
                />
            )}
        </form.Subscribe>
    )
}

it("keeps unique input identities and focus while editing and moving specification rows", () => {
    render(<Harness />)
    const names = screen.getAllByRole("textbox", { name: "规格名称" })
    const nameId = names[0].id
    act(() => names[0].focus())
    fireEvent.change(names[0], { target: { value: "Color" } })
    expect(document.activeElement).toBe(names[0])
    expect(names[0].id).toBe(nameId)
    const value = screen.getByRole("textbox", { name: "Color的第 1 个值" })
    act(() => value.focus())
    fireEvent.change(value, { target: { value: "Red" } })
    expect(document.activeElement).toBe(value)
    fireEvent.click(screen.getByRole("button", { name: "规格项 1 下移" }))
    expect(screen.getAllByRole("textbox", { name: "规格名称" })[1]).toBe(
        names[0],
    )
    const ids = [...document.querySelectorAll("input,button")].map(
        (element) => element.id,
    )
    expect(ids.every(Boolean)).toBe(true)
    expect(new Set(ids).size).toBe(ids.length)
})
