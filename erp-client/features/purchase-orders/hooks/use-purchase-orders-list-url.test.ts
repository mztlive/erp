import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook, act } from "@testing-library/react"

const mockRouter = { push: vi.fn(), replace: vi.fn(), back: vi.fn() }
const mockUseSearchParams = vi.fn(() => new URLSearchParams())

vi.mock("next/navigation", () => ({
    useRouter: () => mockRouter,
    useSearchParams: () => mockUseSearchParams(),
    usePathname: () => "/procurement/orders",
    useParams: () => ({}),
}))

import { usePurchaseOrdersListUrl } from "./use-purchase-orders-list-url"

beforeEach(() => {
    vi.clearAllMocks()
    mockUseSearchParams.mockReturnValue(new URLSearchParams())
})

describe("usePurchaseOrdersListUrl", () => {
    it("空参数时返回默认状态与默认查询输入", () => {
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.url).toEqual({
            q: undefined,
            status: "all",
            metric: "all",
            page: 1,
            pageSize: 20,
            sort: undefined,
            basisId: undefined,
            salesOrderId: undefined,
        })
        expect(result.current.listQueryInput).toEqual({
            q: undefined,
            status: "all",
            metric: "all",
            page: 1,
            pageSize: 20,
            sortBy: undefined,
            sortDir: undefined,
        })
        expect(result.current.search).toBe("")
        expect(result.current.statusFilter).toBe("all")
        expect(result.current.metricKey).toBe("all")
        expect(result.current.effectiveMetric).toBe("all")
        expect(result.current.listReturnHref).toBe("/procurement/orders")
    })

    it("解析排序参数为 sortBy/sortDir", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("sort=amount:desc"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.sortBy).toBe("amount")
        expect(result.current.sortDir).toBe("desc")
        expect(result.current.listQueryInput.sortBy).toBe("amount")
        expect(result.current.listQueryInput.sortDir).toBe("desc")
    })

    it("非法排序回退为空", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("sort=amount:sideways"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.sortBy).toBeUndefined()
        expect(result.current.sortDir).toBeUndefined()
    })

    it("metric=pending_create 时查询输入按全部列表处理", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("metric=pending_create"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.metricKey).toBe("pending_create")
        expect(result.current.effectiveMetric).toBe("all")
        expect(result.current.listQueryInput.metric).toBe("all")
    })

    it("pushUrl 把补丁写回地址栏（跳过默认值）", () => {
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        act(() => {
            result.current.pushUrl({ status: "DRAFT", page: 2 })
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders?status=DRAFT&page=2",
            { scroll: false },
        )
    })

    it("pushUrl 清除参数时输出最小 URL", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=DRAFT"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        act(() => {
            result.current.pushUrl({ q: undefined, status: "all", page: 1 })
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders",
            { scroll: false },
        )
    })

    it("listReturnHref 保留筛选但剔除 basisId", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("status=DRAFT&basisId=bas_1"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.listReturnHref).toBe(
            "/procurement/orders?status=DRAFT",
        )
    })

    it("basisFromUrl 来自 URL 参数", () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("basisId=bas_9"),
        )
        const { result } = renderHook(() => usePurchaseOrdersListUrl())
        expect(result.current.basisFromUrl).toBe("bas_9")
    })
})
