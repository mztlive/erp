import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"

afterEach(cleanup)

test("主行提供查询和更多筛选，常用条件常驻，更多面板与已生效标签按口径展示", () => {
    const onSubmit = vi.fn()
    const onToggleMore = vi.fn()
    const onClearChip = vi.fn()
    const onClearAll = vi.fn()
    const onResetMore = vi.fn()

    const { rerender } = render(
        <ListWorkspaceFilterBar
            idPrefix="filter-bar-test"
            formAriaLabel="列表查询"
            onSubmit={onSubmit}
            search={
                <ListSearchField
                    id="filter-bar-test-search"
                    value="茶"
                    onChange={() => undefined}
                    placeholder="搜索"
                />
            }
            moreOpen={false}
            moreCount={1}
            onToggleMore={onToggleMore}
            onResetMore={onResetMore}
            commonFilters={<span>常用条件</span>}
            morePanel={
                <ListWorkspaceFilterField label="品牌">
                    <input id="filter-bar-test-brand" />
                </ListWorkspaceFilterField>
            }
            resultStatus="共 8 条"
            chips={[{ key: "brand", label: "品牌：测试" }]}
            onClearChip={onClearChip}
            onClearAll={onClearAll}
            hasPendingChanges
            pendingHint="条件已修改，待查询"
            idleHint="结果与当前查询条件一致"
        />,
    )

    fireEvent.submit(screen.getByRole("form", { name: "列表查询" }))
    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(screen.getByRole("button", { name: "查询" })).toBeTruthy()
    expect(screen.getByText("常用条件")).toBeTruthy()
    expect(screen.getByText("共 8 条")).toBeTruthy()
    expect(screen.getByText("已生效")).toBeTruthy()
    expect(screen.getByText("条件已修改，待查询")).toBeTruthy()
    expect(screen.queryByLabelText("更多筛选条件")).toBeNull()

    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    expect(onToggleMore).toHaveBeenCalledTimes(1)

    rerender(
        <ListWorkspaceFilterBar
            idPrefix="filter-bar-test"
            formAriaLabel="列表查询"
            onSubmit={onSubmit}
            search={
                <ListSearchField
                    id="filter-bar-test-search"
                    value="茶"
                    onChange={() => undefined}
                    placeholder="搜索"
                />
            }
            moreOpen
            moreCount={1}
            onToggleMore={onToggleMore}
            onResetMore={onResetMore}
            commonFilters={<span>常用条件</span>}
            morePanel={
                <ListWorkspaceFilterField label="品牌">
                    <input id="filter-bar-test-brand" />
                </ListWorkspaceFilterField>
            }
            resultStatus="共 8 条"
            chips={[{ key: "brand", label: "品牌：测试" }]}
            onClearChip={onClearChip}
            onClearAll={onClearAll}
            idleHint="结果与当前查询条件一致"
        />,
    )

    expect(screen.getByLabelText("更多筛选条件")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "重置更多条件" }))
    expect(onResetMore).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByRole("button", { name: "清除全部" }))
    expect(onClearAll).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByRole("button", { name: "移除品牌：测试" }))
    expect(onClearChip).toHaveBeenCalledWith("brand")
    expect(screen.getByText("结果与当前查询条件一致")).toBeTruthy()
})
