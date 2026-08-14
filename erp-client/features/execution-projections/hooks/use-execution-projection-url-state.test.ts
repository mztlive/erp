import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook, act } from "@testing-library/react"

vi.mock("next/navigation", () => ({
    useRouter: vi.fn(() => ({
        push: vi.fn(),
        replace: vi.fn(),
        back: vi.fn(),
    })),
    useSearchParams: vi.fn(() => new URLSearchParams()),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"
import { useExecutionProjectionUrlState } from "./use-execution-projection-url-state"

const mockedUseSearchParams = vi.mocked(useSearchParams)
const mockedUsePathname = vi.mocked(usePathname)
const mockedUseRouter = vi.mocked(useRouter)

const mockReplace = vi.fn()

function params(raw: string): ReadonlyURLSearchParams {
    return new URLSearchParams(raw) as unknown as ReadonlyURLSearchParams
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedUseSearchParams.mockReturnValue(params(""))
    mockedUsePathname.mockReturnValue("/commerce/execution-projections")
    mockedUseRouter.mockReturnValue({
        push: vi.fn(),
        replace: mockReplace,
        back: vi.fn(),
    } as never)
})

function setParams(raw: string) {
    mockedUseSearchParams.mockReturnValue(params(raw))
}

describe("useExecutionProjectionUrlState", () => {
    it("空参数时回落到默认值", () => {
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        expect(result.current).toMatchObject({
            q: "",
            mallId: "all",
            deliveryStatus: "all",
            source: "all",
            latency: "all",
            reconciliation: "all",
            metric: "all",
            projectionId: undefined,
            revisionId: undefined,
            page: 1,
            pageSize: 8,
            hasActiveFilters: false,
        })
    })

    it("解析全部查询参数", () => {
        setParams(
            "q=abc&mall=m1&deliveryStatus=UNKNOWN,FAILED&source=MIGRATION_BASELINE&latency=over_sla&reconciliation=VERSION_MISMATCH&metric=acked&projectionId=p1&revision=r1&page=3&size=20",
        )
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        expect(result.current).toMatchObject({
            q: "abc",
            mallId: "m1",
            deliveryStatus: "UNKNOWN,FAILED",
            source: "MIGRATION_BASELINE",
            latency: "over_sla",
            reconciliation: "VERSION_MISMATCH",
            metric: "acked",
            projectionId: "p1",
            revisionId: "r1",
            page: 3,
            pageSize: 20,
            hasActiveFilters: true,
        })
    })

    it("非法枚举值回落到 all", () => {
        setParams("source=HACK&latency=huge&reconciliation=???&metric=nope")
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        expect(result.current).toMatchObject({
            source: "all",
            latency: "all",
            reconciliation: "all",
            metric: "all",
        })
    })

    it("分页边界：非数字回 1/8，size 上限 50", () => {
        setParams("page=abc&size=999")
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        expect(result.current.page).toBe(1)
        expect(result.current.pageSize).toBe(50)

        setParams("page=0&size=0")
        const second = renderHook(() => useExecutionProjectionUrlState())
        expect(second.result.current.page).toBe(1)
        expect(second.result.current.pageSize).toBe(8)
    })

    it("replaceParams 设置参数并以 replace 写回 URL（scroll: false）", () => {
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        act(() => {
            result.current.replaceParams({ q: "abc", page: "1" })
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=abc&page=1",
            { scroll: false },
        )
    })

    it("replaceParams 删除 null/空串/all 值并保留其余参数", () => {
        setParams("q=abc&mall=m1&page=2")
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        act(() => {
            result.current.replaceParams({
                q: null,
                mall: "all",
                deliveryStatus: "",
                source: "ERP_SALES_REVISION",
            })
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?page=2&source=ERP_SALES_REVISION",
            { scroll: false },
        )
    })

    it("全部删除后写回无查询串的路径", () => {
        setParams("q=abc")
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        act(() => {
            result.current.replaceParams({ q: null })
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections",
            { scroll: false },
        )
    })

    it("setPageState：回第一页删 page，恢复默认 size 删 size", () => {
        setParams("q=x&page=2&size=20")
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        act(() => {
            result.current.setPageState({ pageIndex: 0, pageSize: 8 })
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=x",
            { scroll: false },
        )

        mockReplace.mockClear()
        act(() => {
            result.current.setPageState({ pageIndex: 2, pageSize: 20 })
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=x&page=3&size=20",
            { scroll: false },
        )
    })

    it("clearFilters 只清筛选与分页，保留对象上下文参数", () => {
        setParams(
            "q=x&mall=m1&deliveryStatus=FAILED&source=ERP_SALES_REVISION&latency=normal&reconciliation=MATCHED&metric=acked&page=4&projectionId=p1&revision=r1",
        )
        const { result } = renderHook(() => useExecutionProjectionUrlState())
        act(() => {
            result.current.clearFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?projectionId=p1&revision=r1",
            { scroll: false },
        )
    })
})
