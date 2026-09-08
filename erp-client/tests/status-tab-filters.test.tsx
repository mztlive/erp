import { createRef } from "react"
import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useLifecycleListFilters } from "@/features/master-data/hooks/use-lifecycle-list-filters"
import { useProductListFilters } from "@/features/master-data/hooks/use-product-list-filters"
import { useSupplierListFilters } from "@/features/master-data/hooks/use-supplier-list-filters"
import { usePurchaseOrdersListFilters } from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import { parsePurchaseOrdersSearchParams } from "@/features/purchase-orders/lib/url-state"
import {
    useConnectionListFilters,
    buildConnectionAppliedChips,
} from "@/features/supplier-api-connections/hooks/use-connection-list-filters"
import { parseConnectionsSearchParams } from "@/features/supplier-api-connections/lib/url-state"

const navigation = vi.hoisted(() => ({
    params: new URLSearchParams(),
    replace: vi.fn(),
}))
vi.mock("next/navigation", () => ({
    usePathname: () => "/list",
    useSearchParams: () => navigation.params,
    useRouter: () => ({ replace: navigation.replace }),
}))
beforeEach(() => {
    navigation.params = new URLSearchParams()
    navigation.replace.mockReset()
})
afterEach(cleanup)

function lastQuery() {
    return new URL(navigation.replace.mock.calls.at(-1)![0], "http://localhost")
        .searchParams
}

const lifecycleHooks = [
    [
        "字典和仓库",
        (ref: ReturnType<typeof createRef<HTMLInputElement>>) => {
            const f = useLifecycleListFilters(ref)
            return { ...f, apply: f.applyListFilters }
        },
    ],
    [
        "商品",
        (ref: ReturnType<typeof createRef<HTMLInputElement>>) => {
            const f = useProductListFilters(ref)
            return { ...f, apply: f.applyProductFilters }
        },
    ],
    [
        "供应商",
        (ref: ReturnType<typeof createRef<HTMLInputElement>>) => {
            const f = useSupplierListFilters(ref)
            return { ...f, apply: f.applySupplierFilters }
        },
    ],
] as const

describe.each(lifecycleHooks)("%s 状态 Tab", (_name, useFilters) => {
    it("高亮取实际查询状态，提交搜索和重置更多条件不覆盖 Tab", () => {
        navigation.params = new URLSearchParams(
            "lifecycleStatus=disabled&metricKey=enabled&page=3",
        )
        const ref = createRef<HTMLInputElement>()
        const { result } = renderHook(() => useFilters(ref))
        expect(result.current.metricKey).toBe("disabled")
        expect(result.current.hasPendingChanges).toBe(false)
        act(() => {
            result.current.setSearchDraft("测试")
            result.current.resetMoreFilters()
        })
        act(() => result.current.apply())
        expect(lastQuery().get("lifecycleStatus")).toBe("disabled")
        expect(lastQuery().get("q")).toBe("测试")
        expect(lastQuery().has("page")).toBe(false)
    })
    it("切换 Tab 保留其他查询，刷新后仍选中同一状态，清除全部回到全部", () => {
        navigation.params = new URLSearchParams(
            "q=商品&revisionTiming=current&page=3",
        )
        const ref = createRef<HTMLInputElement>()
        const { result, rerender } = renderHook(() => useFilters(ref))
        act(() => result.current.changeLifecycle("enabled"))
        expect(lastQuery().get("q")).toBe("商品")
        expect(lastQuery().get("revisionTiming")).toBe("current")
        navigation.params = lastQuery()
        rerender()
        expect(result.current.metricKey).toBe("enabled")
        act(() => result.current.clearAllFilters())
        expect(lastQuery().has("lifecycleStatus")).toBe(false)
    })
})

it.each([
    ["draft", "DRAFT"],
    ["review", "PENDING_REVIEW"],
    ["fulfill", "EFFECTIVE"],
    ["gate_blocked", "all"],
    ["pending_create", "all"],
])("采购旧链接 %s 归一到 %s", (metric, status) => {
    expect(
        parsePurchaseOrdersSearchParams(new URLSearchParams({ metric })),
    ).toMatchObject({ status, metric: "all" })
})
it.each([
    "DRAFT",
    "PENDING_REVIEW",
    "EFFECTIVE",
    "PARTIAL",
    "COMPLETED",
    "VOID",
])("采购 %s Tab 下提交搜索保持状态及导航上下文", (status) => {
    navigation.params = new URLSearchParams({
        status,
        salesOrderId: "sales-1",
        page: "4",
    })
    const ref = createRef<HTMLInputElement>()
    const { result } = renderHook(() => usePurchaseOrdersListFilters(ref))
    act(() => result.current.setSearchDraft("PO"))
    act(() => result.current.applyFilters())
    expect(lastQuery().get("status")).toBe(status)
    expect(lastQuery().get("salesOrderId")).toBe("sales-1")
    expect(lastQuery().get("q")).toBe("PO")
    expect(
        result.current.appliedChips.some((chip) =>
            chip.label.startsWith("状态："),
        ),
    ).toBe(false)
})
it("API 连接状态独立于健康和目录筛选，提交时不覆盖当前状态", () => {
    const state = parseConnectionsSearchParams(
        new URLSearchParams(
            "status=DISABLED&health=FAILED&catalogFreshness=STALE",
        ),
    )
    const patch = vi.fn()
    const { result } = renderHook(() => useConnectionListFilters(state, patch))
    expect(result.current.applied.status).toBe("DISABLED")
    expect(buildConnectionAppliedChips(state).map((chip) => chip.key)).toEqual([
        "health",
        "catalogFreshness",
    ])
    act(() => result.current.setSearchDraft("连接"))
    act(() => result.current.applyFilters())
    expect(patch).toHaveBeenLastCalledWith(
        expect.objectContaining({
            q: "连接",
            health: "FAILED",
            catalogFreshness: "STALE",
            page: 1,
        }),
    )
    expect(patch.mock.calls.at(-1)![0]).not.toHaveProperty("status")
})
