import { afterEach, expect, it } from "vitest"
import { cleanup, render, screen } from "@testing-library/react"
import { ResponsibleUserFilter } from "./responsible-user-filter"
afterEach(cleanup)

it("同名身份独立展示，停用人员与不可用条件仍可见，控件不展示内部 ID", () => {
    render(
        <ResponsibleUserFilter
            id="owner-query"
            value="user-1,user-2,missing"
            onChange={() => {}}
            options={[
                { value: "user-1", label: "张三（zhang-a）" },
                { value: "user-2", label: "张三（zhang-b） · 已停用" },
            ]}
        />,
    )
    expect(screen.getByText("张三（zhang-a）")).toBeTruthy()
    expect(screen.getByText("张三（zhang-b） · 已停用")).toBeTruthy()
    expect(screen.getByText("已选人员（当前不可用）")).toBeTruthy()
    expect(document.getElementById("owner-query")).toBeTruthy()
    expect(document.body.textContent).not.toContain("user-1")
})
