import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"
import { DictionaryCompactList } from "./dictionary-compact-list"
import { UnitOfMeasurePreviewSheet } from "../unit-of-measure/unit-of-measure-preview-sheet"
import { mapUnitOfMeasureRow } from "@/features/master-data/api/list-mappers"

const row = mapUnitOfMeasureRow({
    id: "unit-ci",
    unit_code: "CI",
    name: "次",
    symbol: "次",
    quantity_scale: 0,
    status: "active",
    created_at: 0,
    version: 1,
})
const props = {
    id: "units",
    rows: [row],
    pagination: { pageIndex: 0, pageSize: 20 },
    onPaginationChange: vi.fn(),
    loading: false,
    listLoadFailed: false,
    error: null,
    onRetry: vi.fn(),
    hasActiveFilters: false,
    onClearFilters: vi.fn(),
    emptyTitle: "还没有计量单位资料",
    emptyDescription: "新建计量单位",
    codeLabel: "单位代码",
    selectedId: null,
    onPreview: vi.fn(),
}
afterEach(() => {
    cleanup()
    vi.clearAllMocks()
})

test("单位只提供整行预览入口，代码不重复展示，也不在列表提供停用", () => {
    render(<DictionaryCompactList {...props} />)
    expect(screen.queryByRole("table")).toBeNull()
    expect(screen.getAllByText("CI")).toHaveLength(1)
    expect(screen.queryByRole("button", { name: "停用" })).toBeNull()
    fireEvent.click(
        screen.getByRole("button", { name: "查看次，单位代码：CI，当前启用" }),
    )
    expect(props.onPreview).toHaveBeenCalledWith(row)
})

test("第二页只展示当页资料，翻页回调保留每页数量", () => {
    const rows = Array.from({ length: 21 }, (_, i) => ({
        ...row,
        stableId: `unit-${i}`,
        name: `单位${i}`,
    }))
    render(
        <DictionaryCompactList
            {...props}
            rows={rows}
            pagination={{ pageIndex: 1, pageSize: 20 }}
        />,
    )
    expect(screen.getByRole("button", { name: /查看单位20/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /查看单位0，/ })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "上一页" }))
    expect(props.onPaginationChange).toHaveBeenCalledWith({
        pageIndex: 0,
        pageSize: 20,
    })
})

test("刷新后结果变少会修正越界页码，继续展示剩余资料", () => {
    render(
        <DictionaryCompactList
            {...props}
            pagination={{ pageIndex: 2, pageSize: 20 }}
        />,
    )
    expect(screen.getByRole("button", { name: /查看次/ })).toBeTruthy()
    expect(props.onPaginationChange).toHaveBeenCalledWith({
        pageIndex: 0,
        pageSize: 20,
    })
})

test("筛选空态可以清除条件，查询失败可以重试", () => {
    const { rerender } = render(
        <DictionaryCompactList {...props} rows={[]} hasActiveFilters />,
    )
    fireEvent.click(screen.getByRole("button", { name: "清除筛选" }))
    expect(props.onClearFilters).toHaveBeenCalledOnce()
    rerender(
        <DictionaryCompactList
            {...props}
            rows={[]}
            listLoadFailed
            error={{ kind: "Network", message: "查询失败", retryable: true }}
        />,
    )
    fireEvent.click(screen.getByRole("button", { name: /重试/ }))
    expect(props.onRetry).toHaveBeenCalledOnce()
})

test("预览保留数量小数位零值，受限操作禁用并说明原因", () => {
    const restricted = {
        ...row,
        allowedActions: [],
        actionBlockers: [
            {
                action: "DISABLE",
                code: "NO_PERMISSION",
                message: "没有停用权限",
            },
            {
                action: "CREATE_REVISION",
                code: "NO_PERMISSION",
                message: "没有修改权限",
            },
        ],
    }
    render(
        <UnitOfMeasurePreviewSheet
            row={restricted}
            lastFocusedRowId={{ current: null }}
            onClose={vi.fn()}
            onRevise={vi.fn()}
            onDisable={vi.fn()}
        />,
    )
    expect(screen.getByText("0")).toBeTruthy()
    const disable = screen.getByRole<HTMLButtonElement>("button", {
        name: "停用",
    })
    const revise = screen.getByRole<HTMLButtonElement>("button", {
        name: "更新资料",
    })
    expect(disable.disabled).toBe(true)
    expect(disable.title).toBe("没有停用权限")
    expect(revise.disabled).toBe(true)
    expect(revise.title).toBe("没有修改权限")
})
