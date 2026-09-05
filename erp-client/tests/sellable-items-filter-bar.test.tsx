import * as React from "react"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest"
import { SellableItemsFilterBar } from "@/features/master-data/components/list/sellable-items-filter-bar"
import { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"

const { replace } = vi.hoisted(() => ({
    replace: vi.fn((url: string) => {
        window.history.replaceState(null, "", url)
        window.dispatchEvent(new Event("popstate"))
    }),
}))
vi.mock("next/navigation", () => ({
    useRouter: () => ({ replace }),
    usePathname: () => "/master-data/sellable-items",
    useSearchParams: () => {
        const search = React.useSyncExternalStore(
            (notify) => {
                window.addEventListener("popstate", notify)
                return () => window.removeEventListener("popstate", notify)
            },
            () => window.location.search,
        )
        return React.useMemo(() => new URLSearchParams(search), [search])
    },
}))
beforeAll(() => {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    HTMLElement.prototype.scrollIntoView = function () {}
})
afterEach(cleanup)
beforeEach(() => {
    replace.mockClear()
    window.history.replaceState(null, "", "/master-data/sellable-items")
})
function Harness() {
    const ref = React.useRef<HTMLInputElement>(null)
    const filters = useSellableListFilters(ref)
    return (
        <SellableItemsFilterBar
            searchInputRef={ref}
            filters={filters}
            appliedChips={
                filters.productBrandId
                    ? [{ key: "productBrandId", label: "品牌：测试品牌" }]
                    : []
            }
            filterOptions={{ isPending: false, data: undefined }}
            resultCount={8}
            loading={false}
            failed={false}
        />
    )
}
function input(id: string, value: string) {
    fireEvent.change(document.getElementById(`sellable-items-filter-${id}`)!, {
        target: { value },
    })
}
async function submit() {
    await act(async () => {
        fireEvent.submit(screen.getByRole("form", { name: "公司商品池查询" }))
    })
}

test("编辑不会提前查询，查询统一提交常用和更多条件并回到第一页", async () => {
    window.history.replaceState(null, "", "/master-data/sellable-items?page=3")
    render(<Harness />)
    input("search", " 茶礼 ")
    fireEvent.click(screen.getByRole("radio", { name: "实物" }))
    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    input("region", "北京")
    expect(replace).not.toHaveBeenCalled()
    expect(screen.getByText(/条件已修改，待查询/)).toBeTruthy()
    await submit()
    const params = new URLSearchParams(window.location.search)
    expect(params.get("q")).toBe("茶礼")
    expect(params.get("productKind")).toBe("PHYSICAL")
    expect(params.get("supplyRegion")).toBe("北京")
    expect(params.has("page")).toBe(false)
    expect(screen.queryByText(/条件已修改，待查询/)).toBeNull()
})

test("重置更多条件只清草稿，保留常用条件和已生效结果", () => {
    window.history.replaceState(
        null,
        "",
        "/master-data/sellable-items?q=茶&productKind=PHYSICAL&productBrandId=brand&supplyRegion=北京",
    )
    render(<Harness />)
    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    fireEvent.click(screen.getByRole("button", { name: "重置更多条件" }))
    expect(replace).not.toHaveBeenCalled()
    expect(window.location.search).toContain("productBrandId=brand")
    expect(screen.getByText("品牌：测试品牌")).toBeTruthy()
    expect(
        (
            document.getElementById(
                "sellable-items-filter-search",
            ) as HTMLInputElement
        ).value,
    ).toBe("茶")
    expect(
        screen
            .getByRole("radio", { name: "实物" })
            .getAttribute("aria-checked"),
    ).toBe("true")
    expect(screen.getByText(/条件已修改，待查询/)).toBeTruthy()
})

test("移除已生效标签不会丢弃其他待查询输入", () => {
    window.history.replaceState(
        null,
        "",
        "/master-data/sellable-items?productBrandId=brand",
    )
    render(<Harness />)
    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    input("region", "上海")
    fireEvent.click(screen.getByRole("button", { name: "移除品牌：测试品牌" }))
    expect(window.location.search).toBe("")
    expect(
        (
            document.getElementById(
                "sellable-items-filter-region",
            ) as HTMLInputElement
        ).value,
    ).toBe("上海")
    expect(screen.getByText(/条件已修改，待查询/)).toBeTruthy()
})

test("隐藏的无效价格阻止查询并展开错误字段", async () => {
    render(<Harness />)
    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    input("price-min", "200")
    input("price-max", "100")
    fireEvent.click(screen.getByRole("button", { name: /更多筛选/ }))
    await submit()
    expect(replace).not.toHaveBeenCalled()
    expect(screen.getByRole("alert")).toBeTruthy()
    expect(document.activeElement?.id).toBe("sellable-items-filter-price-min")
})

test("外部 URL 变化会回填条件，清除全部同时清除草稿和当前查询", () => {
    render(<Harness />)
    act(() => {
        window.history.replaceState(
            null,
            "",
            "/master-data/sellable-items?q=茶&productBrandId=brand&page=2",
        )
        window.dispatchEvent(new Event("popstate"))
    })
    expect(
        (
            document.getElementById(
                "sellable-items-filter-search",
            ) as HTMLInputElement
        ).value,
    ).toBe("茶")
    input("search", "新条件")
    fireEvent.click(screen.getByRole("button", { name: "清除全部" }))
    expect(window.location.search).toBe("")
    expect(
        (
            document.getElementById(
                "sellable-items-filter-search",
            ) as HTMLInputElement
        ).value,
    ).toBe("")
    expect(screen.queryByText(/条件已修改，待查询/)).toBeNull()
})
