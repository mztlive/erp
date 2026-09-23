import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"

const queries = vi.hoisted(() => ({
    list: {
        isError: false,
        isFetching: false,
        error: null as unknown,
        data: {
            pages: [
                {
                    items: [
                        {
                            id: "s1",
                            name: "旧销售姓名",
                            account: "sales1",
                            status: "active",
                        },
                    ],
                    total: 1,
                },
            ],
        },
        refetch: vi.fn(),
    },
    selected: {
        isError: false,
        isSuccess: true,
        isFetching: false,
        error: null as unknown,
        data: {
            items: [
                {
                    id: "s1",
                    name: "旧销售姓名",
                    account: "sales1",
                    status: "active",
                },
            ],
        },
        refetch: vi.fn(),
    },
}))
vi.mock("@/features/entity-selectors/hooks/person-directory", () => ({
    usePersonDirectoryList: () => queries.list,
    usePersonDirectorySelected: () => queries.selected,
}))
vi.mock("@/components/business/multi-option-combobox", () => ({
    MultiOptionCombobox: ({
        options,
        value,
    }: {
        options: { value: string; label: string }[]
        value: string[]
    }) => (
        <div data-testid="candidates" data-selected={value.join(",")}>
            {options.map((option) => (
                <span key={option.value}>{option.label}</span>
            ))}
        </div>
    ),
}))
afterEach(cleanup)
beforeEach(() => {
    queries.list.isFetching = false
    queries.list.isError = false
    queries.list.error = null
    queries.selected.isFetching = false
    queries.selected.isError = false
    queries.selected.isSuccess = true
    queries.selected.error = null
})

test("重验人员授权时撤下旧姓名，保留已选 ID", () => {
    const onChange = vi.fn()
    const { rerender } = render(
        <PersonDirectoryFilter
            id="sales"
            category="sales"
            value="s1"
            onChange={onChange}
        />,
    )
    expect(screen.getByText("旧销售姓名 · sales1")).toBeTruthy()
    queries.list.isFetching = true
    queries.selected.isFetching = true
    rerender(
        <PersonDirectoryFilter
            id="sales"
            category="sales"
            value="s1"
            onChange={onChange}
        />,
    )
    expect(screen.queryByText(/旧销售姓名/)).toBeNull()
    expect(screen.getByTestId("candidates").getAttribute("data-selected")).toBe(
        "s1",
    )
    expect(onChange).not.toHaveBeenCalled()
})

test("人员目录撤权后不回显旧缓存姓名，也不静默清除筛选", () => {
    queries.list.isError = true
    queries.list.error = { status: 403 }
    queries.selected.isError = true
    queries.selected.isSuccess = false
    queries.selected.error = { status: 403 }
    const onChange = vi.fn()
    render(
        <PersonDirectoryFilter
            id="sales"
            category="sales"
            value="s1"
            onChange={onChange}
        />,
    )
    expect(screen.queryByText(/旧销售姓名/)).toBeNull()
    expect(screen.getAllByRole("alert")).toHaveLength(2)
    expect(screen.getByTestId("candidates").getAttribute("data-selected")).toBe(
        "s1",
    )
    expect(onChange).not.toHaveBeenCalled()
})
