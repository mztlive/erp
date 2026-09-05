import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
    WorkspaceFamilyNav,
    WorkspaceQueueToolbar,
} from "./workspace-filter-bar"

afterEach(cleanup)

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

test("队列搜索栏保留输入与排序 id，回车提交查询", () => {
    const onSearch = vi.fn()
    render(
        <WorkspaceQueueToolbar
            urlState={{ view: "inbox", sort: "priority_due" }}
            searchDraft="SO"
            onSearchDraftChange={vi.fn()}
            onSortChange={vi.fn()}
            onSearch={onSearch}
            resultCount={3}
        />,
    )

    const search = document.getElementById(
        "workspace-queue-toolbar-search-input",
    )
    expect(search).toBeTruthy()
    expect(
        document.getElementById("workspace-queue-toolbar-sort-trigger"),
    ).toBeTruthy()
    expect(screen.getByRole("button", { name: "查询" })).toBeTruthy()
    expect(screen.getByText("共 3 条待办")).toBeTruthy()
    fireEvent.submit(search!.closest("form") as HTMLFormElement)
    expect(onSearch).toHaveBeenCalledTimes(1)
})
