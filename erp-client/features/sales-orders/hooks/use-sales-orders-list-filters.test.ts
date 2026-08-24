import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { parseSalesOrdersSearchParams } from "@/features/sales-orders/lib/url-state"
import {
    filterDraftFromUrl,
    hasStructuredSalesOrdersFilters,
    resolveSalesOrdersListFilterPatch,
    salesOrdersListFilterDescription,
    salesOrdersListFiltersActive,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import { useSalesOrdersListFilters } from "./use-sales-orders-list-filters"

const makeUrl = (raw = "") =>
    parseSalesOrdersSearchParams(new URLSearchParams(raw))

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useSalesOrdersListFilters", () => {
    it("默认状态下草稿为空且面板收起", () => {
        const pushUrl = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(makeUrl(), pushUrl),
        )

        expect(result.current.searchDraft).toBe("")
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.filterDraft).toEqual({
            customerId: "",
            contractId: "",
            createdBy: "",
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: "",
            createdTo: "",
        })
    })

    it("URL 带结构化筛选时草稿回填并展开面板", () => {
        const url = makeUrl(
            "nature=card_voucher&commercialStatus=draft&customerId=c-1&createdFrom=2026-01-02",
        )
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, vi.fn()),
        )

        expect(result.current.filterPanelOpen).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
        expect(result.current.filterDraft.nature).toBe("card_voucher")
        expect(result.current.filterDraft.customerId).toBe("c-1")
        expect(result.current.filterDraft.createdFrom).toBe("2026-01-02")
    })

    it("applyFilters 落定草稿到 URL（含 trim、空值清理与页码重置）", () => {
        const pushUrl = vi.fn()
        const url = makeUrl()
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.setSearchDraft("  SO-1  ")
        })
        act(() => {
            result.current.setFilterDraft((draft) => ({
                ...draft,
                nature: "card_voucher",
            }))
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(pushUrl).toHaveBeenCalledTimes(1)
        expect(pushUrl).toHaveBeenCalledWith({
            search: "SO-1",
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "card_voucher",
            summary: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    })

    it("反向日期区间在落定时自动交换", () => {
        const pushUrl = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(makeUrl(), pushUrl),
        )

        act(() => {
            result.current.setFilterDraft((draft) => ({
                ...draft,
                createdFrom: "2026-03-02",
                createdTo: "2026-03-01",
            }))
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(pushUrl).toHaveBeenCalledWith(
            expect.objectContaining({
                createdFrom: "2026-03-01",
                createdTo: "2026-03-02",
            }),
        )
    })

    it("草稿与固定工作视图同字段冲突时回退为全部视图", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("summary=mine")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.setFilterDraft((draft) => ({
                ...draft,
                createdBy: "u-1",
            }))
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(pushUrl).toHaveBeenCalledWith(
            expect.objectContaining({ summary: "all", createdBy: "u-1" }),
        )
    })

    it("clearFilters 清空草稿与全部结构化筛选", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("nature=card_voucher&q=SO")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.clearFilters()
        })

        expect(result.current.searchDraft).toBe("")
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.filterDraft).toEqual(
            expect.objectContaining({
                nature: "all",
                createdFrom: "",
                createdTo: "",
            }),
        )
        expect(pushUrl).toHaveBeenCalledWith({
            search: undefined,
            summary: "all",
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    })

    it("URL 变化时草稿重同步但不强制展开面板", () => {
        const pushUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ url }: { url: ReturnType<typeof makeUrl> }) =>
                useSalesOrdersListFilters(url, pushUrl),
            { initialProps: { url: makeUrl() } },
        )

        rerender({ url: makeUrl("q=SO-9&nature=card_voucher") })

        expect(result.current.searchDraft).toBe("SO-9")
        expect(result.current.filterDraft.nature).toBe("card_voucher")
        // 回填只同步草稿，不得抢夺用户当前的面板展开态
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("applyFilters 成功后收起面板", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("nature=card_voucher")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        expect(result.current.filterPanelOpen).toBe(true)
        act(() => {
            result.current.applyFilters()
        })
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("resetMoreFilters 只清结构化条件，保留关键词与工作视图并保持面板展开", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("q=SO&summary=mine&nature=card_voucher")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.resetMoreFilters()
        })

        expect(result.current.filterPanelOpen).toBe(true)
        expect(pushUrl).toHaveBeenCalledWith({
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    })

    it("removeFilter 只移除单个已生效条件并回第 1 页", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("q=SO&nature=card_voucher&commercialStatus=draft")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.removeFilter("commercialStatus")
        })
        expect(pushUrl).toHaveBeenCalledWith({
            commercialStatus: "all",
            page: 1,
        })

        act(() => {
            result.current.removeFilter("search")
        })
        expect(pushUrl).toHaveBeenCalledWith({ search: undefined, page: 1 })

        act(() => {
            result.current.removeFilter("summary")
        })
        expect(pushUrl).toHaveBeenCalledWith({ summary: "all", page: 1 })

        act(() => {
            result.current.removeFilter("createdDate")
        })
        expect(pushUrl).toHaveBeenCalledWith({
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    })

    it("移除客户条件时一并移除依赖客户的合同条件", () => {
        const pushUrl = vi.fn()
        const url = makeUrl("customerId=c-1&contractId=ct-1")
        const { result } = renderHook(() =>
            useSalesOrdersListFilters(url, pushUrl),
        )

        act(() => {
            result.current.removeFilter("customerId")
        })

        expect(pushUrl).toHaveBeenCalledWith({
            customerId: undefined,
            contractId: undefined,
            page: 1,
        })
    })
})

describe("lib/sales-orders-list-filters 纯函数", () => {
    it("hasStructuredSalesOrdersFilters 识别所有结构化条件", () => {
        expect(hasStructuredSalesOrdersFilters(makeUrl())).toBe(false)
        expect(
            hasStructuredSalesOrdersFilters(makeUrl("nature=card_voucher")),
        ).toBe(true)
        expect(
            hasStructuredSalesOrdersFilters(makeUrl("createdTo=2026-01-01")),
        ).toBe(true)
        expect(hasStructuredSalesOrdersFilters(makeUrl("customerId=c-1"))).toBe(
            true,
        )
    })

    it("salesOrdersListFiltersActive 同时考虑关键词与工作视图", () => {
        expect(salesOrdersListFiltersActive(makeUrl("q=SO"))).toBe(true)
        expect(salesOrdersListFiltersActive(makeUrl("summary=mine"))).toBe(true)
        expect(salesOrdersListFiltersActive(makeUrl())).toBe(false)
    })

    it("filterDraftFromUrl 保留空串占位", () => {
        const draft = filterDraftFromUrl(makeUrl("summary=mine"))
        expect(draft.createdBy).toBe("")
        expect(draft.nature).toBe("all")
    })

    it("resolveSalesOrdersListFilterPatch 处理空白关键词与异常视图冲突", () => {
        const patch = resolveSalesOrdersListFilterPatch({
            summary: "exception",
            searchDraft: "   ",
            filterDraft: {
                ...filterDraftFromUrl(makeUrl()),
                commercialStatus: "draft",
            },
        })
        expect(patch.search).toBeUndefined()
        expect(patch.summary).toBe("all")
        expect(patch.commercialStatus).toBe("draft")
        expect(patch.page).toBe(1)
    })

    it("salesOrdersListFilterDescription 汇总当前筛选文案", () => {
        expect(salesOrdersListFilterDescription(makeUrl())).toBe(
            "设置一个或多个条件后统一搜索；筛选条件会保存在网址中，便于刷新、返回与分享。",
        )
        expect(
            salesOrdersListFilterDescription(
                makeUrl("summary=mine&nature=card_voucher&q=SO-1"),
            ),
        ).toBe("当前筛选：待我处理 · 卡券 · 关键词“SO-1”")
        expect(
            salesOrdersListFilterDescription(
                makeUrl("createdFrom=2026-01-01&createdTo=2026-02-01"),
            ),
        ).toBe("当前筛选：创建日期 2026-01-01 至 2026-02-01")
    })
})
