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
import { useExecutionProjectionFilters } from "./use-execution-projection-filters"

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

describe("useExecutionProjectionFilters", () => {
    it("空参数时草稿全部为默认值，面板收起", () => {
        const { result } = renderHook(() => useExecutionProjectionFilters())
        expect(result.current).toMatchObject({
            q: "",
            searchDraft: "",
            mallIdDraft: "all",
            deliveryStatusDraft: "all",
            latencyDraft: "all",
            reconciliationDraft: "all",
            sourceDraft: "all",
            panelOpen: false,
            hasStructuredFilters: false,
            hasActiveFilters: false,
        })
    })

    it("带结构化条件的深链草稿回填并展开面板", () => {
        setParams(
            "mall=m1&deliveryStatus=FAILED&latency=over_sla&reconciliation=VERSION_MISMATCH&source=MIGRATION_BASELINE",
        )
        const { result } = renderHook(() => useExecutionProjectionFilters())
        expect(result.current).toMatchObject({
            mallIdDraft: "m1",
            deliveryStatusDraft: "FAILED",
            latencyDraft: "over_sla",
            reconciliationDraft: "VERSION_MISMATCH",
            sourceDraft: "MIGRATION_BASELINE",
            panelOpen: true,
            hasStructuredFilters: true,
        })
    })

    it("草稿变化不写 URL，不触发请求", () => {
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.setMallIdDraft("m2")
            result.current.setLatencyDraft("over_sla")
            result.current.setSearchDraft("abc")
        })
        expect(mockReplace).not.toHaveBeenCalled()
    })

    it("applyFilters 一次写入全部筛选并收起面板、回第 1 页", () => {
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.setSearchDraft("  SO-9  ")
            result.current.setMallIdDraft("m1")
            result.current.setDeliveryStatusDraft("UNKNOWN,FAILED")
            result.current.setLatencyDraft("over_sla")
            result.current.setReconciliationDraft("MATCHED")
            result.current.setSourceDraft("ERP_SALES_REVISION")
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=SO-9&mall=m1&deliveryStatus=UNKNOWN%2CFAILED&latency=over_sla&reconciliation=MATCHED&source=ERP_SALES_REVISION",
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it("applyFilters 默认值从 URL 省略", () => {
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.applyFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections",
            { scroll: false },
        )
    })

    it("clearAllFilters 同时重置草稿、面板、全部筛选参数与分页，保留对象上下文", () => {
        setParams(
            "q=x&mall=m1&deliveryStatus=FAILED&latency=normal&reconciliation=MATCHED&source=ERP_SALES_REVISION&metric=acked&page=4&projectionId=p1&revision=r1",
        )
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.clearAllFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?projectionId=p1&revision=r1",
            { scroll: false },
        )
        expect(result.current).toMatchObject({
            searchDraft: "",
            mallIdDraft: "all",
            deliveryStatusDraft: "all",
            latencyDraft: "all",
            reconciliationDraft: "all",
            sourceDraft: "all",
            panelOpen: false,
        })
        // 保留 projectionId / revision 等导航与对象上下文
        const called = mockReplace.mock.calls[0][0] as string
        expect(called).toContain("projectionId=p1")
        expect(called).toContain("revision=r1")
    })

    it("resetMoreFilters 只清结构化条件，保留关键词与指标快捷筛选，面板保持展开", () => {
        setParams("q=x&mall=m1&latency=over_sla&metric=acked")
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.resetMoreFilters()
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=x&metric=acked",
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe("x")
        expect(result.current.metric).toBe("acked")
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.mallIdDraft).toBe("all")
    })

    it("removeFilter 只移除单个条件并同步对应草稿", () => {
        setParams("q=x&mall=m1&metric=acked")
        const { result } = renderHook(() => useExecutionProjectionFilters())
        act(() => {
            result.current.removeFilter("mall")
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/commerce/execution-projections?q=x&metric=acked",
            { scroll: false },
        )
        expect(result.current.mallIdDraft).toBe("all")
        expect(result.current.searchDraft).toBe("x")

        act(() => {
            result.current.removeFilter("q")
        })
        expect(result.current.searchDraft).toBe("")
    })

    it("URL 回填同步草稿但不改变面板展开态", () => {
        const { result, rerender } = renderHook(() =>
            useExecutionProjectionFilters(),
        )
        act(() => {
            result.current.setPanelOpen(true)
        })
        setParams("mall=m1")
        rerender()
        expect(result.current.mallIdDraft).toBe("m1")
        expect(result.current.panelOpen).toBe(true)
    })
})
