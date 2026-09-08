import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
    WorkspaceFamilyNav,
    WorkspaceQueueToolbar,
    WorkspaceQueueScopeNav,
    WorkspaceQueueStatusNav,
} from "./workspace-filter-bar"

afterEach(cleanup)

test("任务类型菜单显示统计快照并可切换采购", async () => {
    const onFamilyChange = vi.fn()
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
            onFamilyChange={onFamilyChange}
        />,
    )

    fireEvent.click(screen.getByRole("button", { name: "任务类型：全部" }))
    expect(
        await screen.findByRole("menuitemradio", { name: "全部 6 项" }),
    ).toBeTruthy()
    fireEvent.click(screen.getByRole("menuitemradio", { name: "采购 2 项" }))
    expect(onFamilyChange).toHaveBeenCalledWith("procurement")
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

test("仅看超期保持待办主视图且可以取消，复选框具有稳定 id", () => {
    const onMetricClick = vi.fn()
    const metrics = [
        {
            key: "inbox" as const,
            label: "待我处理",
            count: 5,
            visible: true,
            tone: "neutral" as const,
        },
        {
            key: "overdue" as const,
            label: "已超期",
            count: 2,
            visible: true,
            tone: "warning" as const,
        },
        {
            key: "started" as const,
            label: "我发起的",
            count: 3,
            visible: true,
            tone: "neutral" as const,
        },
    ]
    render(
        <>
            <WorkspaceQueueScopeNav
                metrics={metrics}
                activeMetric="overdue"
                onMetricClick={onMetricClick}
            />
            <WorkspaceQueueStatusNav
                metrics={metrics}
                activeMetric="overdue"
                onMetricClick={onMetricClick}
            />
        </>,
    )
    expect(
        screen
            .getByRole("button", { name: "待我处理 5 项" })
            .getAttribute("aria-pressed"),
    ).toBe("true")
    const overdue = screen.getByRole("checkbox", {
        name: /仅看超期/,
    }) as HTMLInputElement
    expect(overdue.checked).toBe(true)
    expect(overdue.id).toBe("workspace-queue-scope-overdue")
    fireEvent.click(overdue)
    expect(onMetricClick).toHaveBeenCalledWith("inbox")
})

test("关键词可直接清除，排序按钮展示当前排序", () => {
    const onClearSearch = vi.fn()
    render(
        <WorkspaceQueueToolbar
            urlState={{ view: "inbox", sort: "due_asc", query: "SO" }}
            searchDraft="SO"
            onSearchDraftChange={vi.fn()}
            onSortChange={vi.fn()}
            onSearch={vi.fn()}
            onClearSearch={onClearSearch}
        />,
    )
    expect(
        screen.getByRole("button", { name: "排序：截止时间" }).textContent,
    ).toContain("截止时间")
    fireEvent.click(screen.getByRole("button", { name: "清除关键词" }))
    expect(onClearSearch).toHaveBeenCalledTimes(1)
})
