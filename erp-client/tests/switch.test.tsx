import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { Switch } from "@/components/ui/switch"

describe("Switch", () => {
    it("allows a labelled stable control to toggle without submitting its form", () => {
        const submitted = vi.fn()
        function Form() {
            const [checked, setChecked] = useState(true)
            return (
                <form onSubmit={submitted}>
                    <label htmlFor="responsibility-enabled">启用规则</label>
                    <Switch
                        id="responsibility-enabled"
                        checked={checked}
                        onCheckedChange={setChecked}
                    />
                </form>
            )
        }
        render(<Form />)
        const control = screen.getByRole("switch", { name: "启用规则" })
        expect(document.getElementById("responsibility-enabled")).toBe(control)
        expect(control.getAttribute("aria-checked")).toBe("true")
        fireEvent.click(control)
        expect(control.getAttribute("aria-checked")).toBe("false")
        fireEvent.click(screen.getByText("启用规则"))
        expect(control.getAttribute("aria-checked")).toBe("true")
        expect(submitted).not.toHaveBeenCalled()
    })
})
