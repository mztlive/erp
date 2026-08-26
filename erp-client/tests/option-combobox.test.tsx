import { fireEvent, render, screen } from "@testing-library/react"
import { beforeAll, expect, test } from "vitest"

import { OptionCombobox } from "@/components/business/option-combobox"

beforeAll(() => {
    class ResizeObserverStub {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    globalThis.ResizeObserver = ResizeObserverStub
    HTMLElement.prototype.scrollIntoView = function () {}
    HTMLElement.prototype.hasPointerCapture = function () {
        return false
    }
    HTMLElement.prototype.releasePointerCapture = function () {}
    HTMLElement.prototype.setPointerCapture = function () {}
})

test("下拉选项展示完整标签，不截成省略号", () => {
    render(
        <OptionCombobox
            options={[
                { value: "direct", label: "云桦有礼 · 供应商直发" },
                { value: "warehouse", label: "云桦有礼 · 入仓" },
            ]}
            value={null}
            onValueChange={() => {}}
            aria-label="履约方案"
        />,
    )

    const trigger = screen.getByRole("combobox", { name: "履约方案" })
    trigger.focus()
    fireEvent.keyDown(trigger, { key: "ArrowDown" })

    const option = screen.getByRole("option", { name: "云桦有礼 · 供应商直发" })
    expect(option.textContent).toContain("云桦有礼 · 供应商直发")
    expect(option.querySelector(".truncate")).toBeNull()
    expect(
        option.querySelector("span")?.className.includes("whitespace-nowrap"),
    ).toBe(true)
})
