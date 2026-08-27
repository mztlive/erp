import { render, screen } from "@testing-library/react"
import { expect, test, vi } from "vitest"

import { WorkspaceFamilyNav } from "./workspace-filter-bar"

test("任务族显示服务端统计且全部数量按统计快照汇总", () => {
    render(
        <WorkspaceFamilyNav
            urlState={{ view: "inbox", sort: "priority_due" }}
            counts={{
                approval: 1,
                procurement: 2,
                fulfillment: 1,
                finance: 2,
                exception: 0,
            }}
            onFamilyChange={vi.fn()}
        />,
    )

    expect(screen.getByRole("button", { name: "全部 6 项" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "采购 2 项" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "异常 0 项" })).toBeTruthy()
})
