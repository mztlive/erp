import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, beforeAll, expect, test } from "vitest"

import { DataTable } from "@/components/business/data-table"
import { ListToolbar } from "@/components/business/list"
import { ListWorkSurface } from "@/components/business/list-workspace"

beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
})

afterEach(cleanup)

function ResultsTable() {
    return (
        <DataTable
            id="filter-layout-results"
            data={[{ id: "one", name: "商品" }]}
            columns={[{ accessorKey: "name", header: "名称" }]}
            getRowId={(row) => row.id}
            rowCount={1}
        />
    )
}

test("列设置挂在搜索主行，展开筛选区域不包含列设置", () => {
    const { container } = render(
        <ListWorkSurface
            ariaLabel="列表"
            toolbar={
                <form>
                    <ListToolbar
                        search={<input id="filter-layout-search" />}
                        secondary={<div>更多筛选条件</div>}
                    />
                </form>
            }
            table={<ResultsTable />}
        />,
    )

    const columnSettings = screen.getByRole("button", { name: "列设置" })
    expect(
        columnSettings.closest('[data-slot="list-toolbar-primary"]'),
    ).not.toBeNull()
    expect(
        columnSettings.closest('[data-slot="list-toolbar-secondary"]'),
    ).toBeNull()
    expect(
        container.querySelector('[data-slot="table-frame-view-options"]')
            ?.childElementCount,
    ).toBe(0)
})

test("自定义工具栏继续在列表原有插槽显示列设置", () => {
    render(
        <ListWorkSurface
            ariaLabel="列表"
            toolbar={<input id="custom-filter-search" />}
            table={<ResultsTable />}
        />,
    )

    expect(
        screen
            .getByRole("button", { name: "列设置" })
            .closest('[data-slot="table-frame-view-options"]'),
    ).not.toBeNull()
})
