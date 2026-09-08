import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeAll, expect, test } from "vitest"

import { DataTable } from "@/components/business/data-table"
import { ListToolbar } from "@/components/business/list"
import {
    ListWorkSurface,
    ListWorkspaceViews,
} from "@/components/business/list-workspace"

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

test("只有一个视图时不渲染 tab 行", () => {
    render(
        <ListWorkspaceViews
            ariaLabel="合同视图"
            hint="选择合同查看详情"
            items={[
                {
                    id: "card-contracts-list-view-all",
                    label: "全部合同",
                    count: 1,
                    active: true,
                    onClick: () => undefined,
                },
            ]}
        />,
    )

    expect(screen.queryByRole("button", { name: "全部合同 1" })).toBeNull()
    expect(screen.queryByText("选择合同查看详情")).toBeNull()
})

test("多个视图时渲染可切换 tab", () => {
    render(
        <ListWorkspaceViews
            ariaLabel="销售单工作视图"
            items={[
                {
                    id: "sales-orders-list-filter-summary-all",
                    label: "全部",
                    count: 12,
                    active: true,
                    onClick: () => undefined,
                },
                {
                    id: "sales-orders-list-filter-summary-mine",
                    label: "我负责的",
                    active: false,
                    onClick: () => undefined,
                },
            ]}
        />,
    )

    expect(screen.getByRole("button", { name: "全部12" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "我负责的" })).toBeTruthy()
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

test("默认摘要列可以显示全部并恢复，不丢失记录或行身份", async () => {
    const { container } = render(
        <DataTable
            id="column-preset-results"
            data={[{ id: "one", name: "商品", reference: "SKU-001" }]}
            columns={[
                { accessorKey: "name", header: "名称", enableHiding: false },
                { accessorKey: "reference", header: "资料编号" },
            ]}
            defaultColumnVisibility={{ reference: false }}
            getRowId={(row) => row.id}
            rowCount={1}
            onRowPreview={() => undefined}
        />,
    )
    expect(screen.queryByText("SKU-001")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "列设置" }))
    fireEvent.click(await screen.findByRole("button", { name: "显示全部列" }))
    await waitFor(() => expect(screen.getByText("SKU-001")).toBeTruthy())
    expect(screen.getByRole("checkbox", { name: "资料编号" })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "恢复默认列" }))
    await waitFor(() => expect(screen.queryByText("SKU-001")).toBeNull())
    expect(
        container.querySelector("#column-preset-results-row-one"),
    ).not.toBeNull()
    expect(container.querySelectorAll("tbody tr")).toHaveLength(1)
    expect(container.querySelectorAll("th")).toHaveLength(2)
})
