import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
    within,
} from "@testing-library/react"
import { afterEach, beforeAll, expect, test } from "vitest"

import { DataTable } from "@/components/business/data-table"
import { BusinessTableFrame, ListToolbar } from "@/components/business/list"
import {
    ListWorkSurface,
    ListWorkspaceViews,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"

beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
})

afterEach(cleanup)

function ResultsTable({ id = "filter-layout-results" }: { id?: string }) {
    return (
        <DataTable
            id={id}
            data={[{ id: "one", name: "商品" }]}
            columns={[{ accessorKey: "name", header: "名称" }]}
            getRowId={(row) => row.id}
            rowCount={1}
        />
    )
}

test("列设置位于表格上方，与搜索和展开筛选区域分离", () => {
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
    ).toBeNull()
    expect(
        columnSettings.closest('[data-slot="list-toolbar-secondary"]'),
    ).toBeNull()
    expect(columnSettings.closest('[data-slot="table-toolbar"]')).not.toBeNull()
    expect(columnSettings.closest("form")).toBeNull()
    expect(
        container
            .querySelector('[data-slot="table-toolbar"]')
            ?.nextElementSibling?.getAttribute("data-slot"),
    ).toBe("business-table-frame-table")
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

test("自定义查询工具栏也使用统一表格工具栏", () => {
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
            .closest('[data-slot="table-toolbar-column-settings"]'),
    ).not.toBeNull()
})

test.each([false, true])(
    "旧列表框架统一使用表格工具栏，showHeader=%s",
    (showHeader) => {
        render(
            <BusinessTableFrame
                title="结果"
                showHeader={showHeader}
                toolbar={<input id="legacy-search" />}
                selectionBar={<span>已选 1 件</span>}
                tableActions={<button id="legacy-view">表格视图</button>}
                table={<ResultsTable />}
            />,
        )
        const toolbar = screen
            .getByRole("button", { name: "列设置" })
            .closest('[data-slot="table-toolbar"]')!
        expect(
            within(toolbar as HTMLElement).getByText("已选 1 件"),
        ).toBeTruthy()
        expect(
            within(toolbar as HTMLElement).getByRole("button", {
                name: "表格视图",
            }),
        ).toBeTruthy()
        expect(toolbar.nextElementSibling?.getAttribute("data-slot")).toBe(
            "business-table-frame-table",
        )
        expect(toolbar.querySelector("#legacy-search")).toBeNull()
        expect(within(toolbar as HTMLElement).queryByText("共 1 条")).toBeNull()
    },
)

test("独立表格和自定义表格操作共用工具栏，列设置固定在右侧插槽", () => {
    render(
        <DataTable
            id="standalone-table"
            data={[{ id: "one", name: "商品" }]}
            columns={[{ accessorKey: "name", header: "名称" }]}
            getRowId={(row) => row.id}
            rowCount={1}
            renderToolbar={() => (
                <button id="standalone-batch">批量处理</button>
            )}
        />,
    )
    const toolbar = screen
        .getByRole("button", { name: "列设置" })
        .closest('[data-slot="table-toolbar"]')!
    expect(
        within(toolbar as HTMLElement).getByRole("button", {
            name: "批量处理",
        }),
    ).toBeTruthy()
    expect(toolbar.nextElementSibling?.getAttribute("data-slot")).toBe(
        "data-table-surface",
    )
})

test("同一框架中的多张表分别提供列设置", () => {
    render(
        <ListWorkSurface
            ariaLabel="多表"
            table={
                <>
                    <ResultsTable id="first-table" />
                    <ResultsTable id="second-table" />
                </>
            }
        />,
    )
    const buttons = screen.getAllByRole("button", { name: "列设置" })
    expect(buttons).toHaveLength(2)
    expect(buttons[0]!.closest('[data-slot="data-table"]')?.id).toBe(
        "first-table",
    )
    expect(buttons[1]!.closest('[data-slot="data-table"]')?.id).toBe(
        "second-table",
    )
})

test("嵌套框架的列设置只进入所属表格工具栏", () => {
    render(
        <ListWorkSurface
            ariaLabel="外层列表"
            table={
                <>
                    <ResultsTable id="outer-table" />
                    <BusinessTableFrame
                        title="内层列表"
                        table={<ResultsTable id="inner-table" />}
                    />
                </>
            }
        />,
    )
    const outer = document.getElementById(
        "outer-table-column-visibility-trigger",
    )!
    const inner = document.getElementById(
        "inner-table-column-visibility-trigger",
    )!
    expect(outer.closest('[data-business-component="table-frame"]')).not.toBe(
        inner.closest('[data-business-component="table-frame"]'),
    )
    expect(screen.getAllByRole("button", { name: "列设置" })).toHaveLength(2)
})

test("表格与卡片切换时清理列设置并保留选择和视图操作", () => {
    function Surface({ gallery }: { gallery: boolean }) {
        return (
            <ListWorkSurface
                ariaLabel="商品池"
                selectionBar={<span>已选 2 件</span>}
                tableActions={<button id="switch-layout">切换视图</button>}
                table={gallery ? <div>商品卡片</div> : <ResultsTable />}
            />
        )
    }
    const { rerender } = render(<Surface gallery={false} />)
    expect(screen.getAllByRole("button", { name: "列设置" })).toHaveLength(1)
    rerender(<Surface gallery />)
    expect(screen.queryByRole("button", { name: "列设置" })).toBeNull()
    expect(screen.getByText("已选 2 件")).toBeTruthy()
    expect(screen.getByRole("button", { name: "切换视图" })).toBeTruthy()
    rerender(<Surface gallery={false} />)
    expect(screen.getAllByRole("button", { name: "列设置" })).toHaveLength(1)
})

test.each([false, true])(
    "关闭列设置或所有列固定时不产生工具栏内容，showColumnVisibility=%s",
    (showColumnVisibility) => {
        const { container } = render(
            <ListWorkSurface
                ariaLabel="固定表格"
                table={
                    <DataTable
                        id="fixed-table"
                        data={[]}
                        columns={[
                            {
                                accessorKey: "name",
                                header: "名称",
                                enableHiding: false,
                            },
                        ]}
                        getRowId={() => "one"}
                        rowCount={0}
                        showColumnVisibility={showColumnVisibility}
                    />
                }
            />,
        )
        expect(screen.queryByRole("button", { name: "列设置" })).toBeNull()
        expect(
            container.querySelectorAll(
                "[data-table-toolbar-content]:not(:empty)",
            ),
        ).toHaveLength(0)
    },
)

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

function CustomerFilters({
    resultStatus = "共 1 个客户",
}: {
    resultStatus?: string
}) {
    return (
        <ListWorkspaceFilterBar
            idPrefix="customer-result-filter"
            formAriaLabel="客户查询"
            onSubmit={() => undefined}
            search={<input id="customer-result-search" />}
            resultStatus={resultStatus}
            chips={[{ key: "status", label: "状态：启用" }]}
            onClearChip={() => undefined}
            hasPendingChanges
        />
    )
}

test("筛选结果与列设置合并为一行，保留筛选提示且不重复总数", () => {
    const { container, rerender } = render(
        <ListWorkSurface
            ariaLabel="客户"
            toolbar={<CustomerFilters />}
            table={<ResultsTable />}
        />,
    )
    const toolbar = screen
        .getByRole("button", { name: "列设置" })
        .closest('[data-slot="table-toolbar"]') as HTMLElement
    expect(within(toolbar).getByText("共 1 个客户")).toBeTruthy()
    expect(within(toolbar).queryByText("共 1 条")).toBeNull()
    expect(within(toolbar).getByText("条件已修改，待查询")).toBeTruthy()
    expect(
        within(toolbar).getByRole("button", { name: "移除状态：启用" }),
    ).toBeTruthy()
    expect(toolbar.closest("form")).toBeNull()
    expect(
        container.querySelectorAll('[data-slot="table-toolbar"]'),
    ).toHaveLength(1)
    expect(
        within(screen.getByRole("form", { name: "客户查询" })).queryByRole(
            "status",
        ),
    ).toBeNull()

    rerender(<ListWorkSurface ariaLabel="客户" table={<ResultsTable />} />)
    expect(screen.queryByText("共 1 个客户")).toBeNull()
    expect(within(toolbar).getByText("共 1 条")).toBeTruthy()
})

test("合并结果行仍保留勾选数量，切换空态和卡片后保留查询状态", () => {
    const { rerender } = render(
        <ListWorkSurface
            ariaLabel="客户"
            toolbar={<CustomerFilters />}
            table={
                <DataTable
                    id="selected-customers"
                    data={[{ id: "one", name: "客户" }]}
                    columns={[{ accessorKey: "name", header: "名称" }]}
                    getRowId={(row) => row.id}
                    rowCount={1}
                    enableRowSelection
                    defaultRowSelection={{ one: true }}
                />
            }
        />,
    )
    expect(screen.getByText("已选 1 / 1 条")).toBeTruthy()
    expect(screen.getByText("共 1 个客户")).toBeTruthy()
    rerender(
        <ListWorkSurface
            ariaLabel="客户"
            toolbar={<CustomerFilters resultStatus="查询未完成" />}
            table={<div>暂无结果</div>}
        />,
    )
    expect(screen.getByText("查询未完成")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "列设置" })).toBeNull()
    expect(screen.queryByText("已选 1 / 1 条")).toBeNull()
})

test("共用筛选的多张表仍分别显示自己的数量与列设置", () => {
    render(
        <ListWorkSurface
            ariaLabel="多表查询"
            toolbar={<CustomerFilters />}
            table={
                <>
                    <ResultsTable id="first-filtered-table" />
                    <ResultsTable id="second-filtered-table" />
                </>
            }
        />,
    )
    for (const id of ["first-filtered-table", "second-filtered-table"]) {
        const toolbar = document
            .getElementById(id)!
            .querySelector('[data-slot="table-toolbar"]') as HTMLElement
        expect(within(toolbar).getByText("共 1 条")).toBeTruthy()
        expect(
            within(toolbar).getByRole("button", { name: "列设置" }),
        ).toBeTruthy()
    }
    expect(screen.getAllByText("共 1 个客户")).toHaveLength(1)
})
