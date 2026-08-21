import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const navigation = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    pathname: "/supplier-api/orders",
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navigation.push,
        replace: navigation.replace,
        back: navigation.back,
    }),
    useSearchParams: () => navigation.searchParams,
    usePathname: () => navigation.pathname,
    useParams: () => ({}),
}))

vi.mock("@/features/entity-selectors/hooks/queries", () => ({
    useSupplierSelectorQuery: () => ({
        list: { data: [] },
        selected: { data: undefined },
    }),
}))

import { useSupplierOrdersFilters } from "./use-supplier-orders-filters"

function renderFilters() {
    const ref = { current: null as HTMLInputElement | null }
    const rendered = renderHook(() => useSupplierOrdersFilters(ref))
    return rendered
}

function urlOf(call: unknown[]): URLSearchParams {
    const raw = call[0] as string
    const qs = raw.includes("?") ? raw.slice(raw.indexOf("?") + 1) : ""
    return new URLSearchParams(qs)
}

beforeEach(() => {
    navigation.push.mockReset()
    navigation.replace.mockReset()
    navigation.pathname = "/supplier-api/orders"
    navigation.searchParams = new URLSearchParams()
})

afterEach(() => {
    cleanup()
})

describe("useSupplierOrdersFilters — panel and drafts", () => {
    it("opens the panel initially only for deep links with structured filters", () => {
        expect(renderFilters().result.current.panelOpen).toBe(false)

        navigation.searchParams = new URLSearchParams("supplierId=sup_1")
        expect(renderFilters().result.current.panelOpen).toBe(true)

        navigation.searchParams = new URLSearchParams(
            "fulfillmentStatus=EXCEPTION",
        )
        expect(renderFilters().result.current.panelOpen).toBe(true)

        navigation.searchParams = new URLSearchParams("q=SFO&aftersalePending=1")
        expect(renderFilters().result.current.panelOpen).toBe(false)
    })

    it("does not request while drafts change", () => {
        const { result } = renderFilters()

        act(() => {
            result.current.setSearchDraft("SFO-9")
            result.current.setSupplierIdDraft("sup_2")
            result.current.setFulfillmentStatusesDraft(["EXCEPTION"])
            result.current.setCancelStatusesDraft(["FAILED"])
            result.current.setRefundStatusesDraft(["MANUAL"])
            result.current.setPaidFromDraft("2026-08-01")
            result.current.setPaidToDraft("2026-08-08")
        })

        expect(navigation.replace).not.toHaveBeenCalled()
    })

    it("applies every draft in one URL patch and closes the panel", () => {
        navigation.searchParams = new URLSearchParams(
            "view=all&sort=identity&dir=desc&page=3",
        )
        const { result } = renderFilters()

        act(() => {
            result.current.setPanelOpen(true)
            result.current.setSearchDraft("SFO-1")
            result.current.setSupplierIdDraft("sup_1")
            result.current.setFulfillmentStatusesDraft(["EXCEPTION"])
            result.current.setCancelStatusesDraft(["FAILED"])
            result.current.setRefundStatusesDraft(["MANUAL"])
            result.current.setPaidFromDraft("2026-08-01")
            result.current.setPaidToDraft("2026-08-08")
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(navigation.replace).toHaveBeenCalledTimes(1)
        const params = urlOf(navigation.replace.mock.calls[0]!)
        expect(params.get("q")).toBe("SFO-1")
        expect(params.get("supplierId")).toBe("sup_1")
        expect(params.get("fulfillmentStatus")).toBe("EXCEPTION")
        expect(params.get("cancelStatus")).toBe("FAILED")
        expect(params.get("refundStatus")).toBe("MANUAL")
        expect(params.get("paidFrom")).toBe("2026-08-01")
        expect(params.get("paidTo")).toBe("2026-08-08")
        expect(params.get("page")).toBeNull()
        expect(params.get("aftersalePending")).toBeNull()
        // 视图与排序保留
        expect(params.get("view")).toBe("all")
        expect(params.get("sort")).toBe("identity")
        expect(params.get("dir")).toBe("desc")
        expect(result.current.panelOpen).toBe(false)
    })

    it("keeps the panel open and writes nothing when the paid range is invalid", () => {
        const { result } = renderFilters()

        act(() => {
            result.current.setPanelOpen(true)
            result.current.setPaidFromDraft("2026-08-08")
            result.current.setPaidToDraft("2026-08-01")
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(navigation.replace).not.toHaveBeenCalled()
        expect(result.current.filterError).toBe("支付开始日期不能晚于结束日期")
        expect(result.current.panelOpen).toBe(true)
    })
})

describe("useSupplierOrdersFilters — clear and reset", () => {
    it("clearAllFilters resets drafts, error, panel, URL filters and pagination, keeping view/sort/navigation", () => {
        navigation.searchParams = new URLSearchParams(
            "q=SFO&supplierId=sup_1&fulfillmentStatus=EXCEPTION&cancelStatus=FAILED&refundStatus=MANUAL&aftersalePending=1&paidFrom=2026-08-01&paidTo=2026-08-08&page=5&view=all&sort=identity&dir=desc&returnTo=%2Fcommerce%2Fconsumption-orders",
        )
        const { result } = renderFilters()

        act(() => {
            result.current.setSearchDraft("手写")
            result.current.setFilterError("旧错误")
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.clearAllFilters()
        })

        expect(result.current.searchDraft).toBe("")
        expect(result.current.supplierIdDraft).toBeNull()
        expect(result.current.fulfillmentStatusesDraft).toEqual([])
        expect(result.current.cancelStatusesDraft).toEqual([])
        expect(result.current.refundStatusesDraft).toEqual([])
        expect(result.current.paidFromDraft).toBe("")
        expect(result.current.paidToDraft).toBe("")
        expect(result.current.filterError).toBeNull()
        expect(result.current.panelOpen).toBe(false)

        const params = urlOf(navigation.replace.mock.calls[0]!)
        for (const key of [
            "q",
            "supplierId",
            "fulfillmentStatus",
            "cancelStatus",
            "refundStatus",
            "aftersalePending",
            "paidFrom",
            "paidTo",
            "page",
        ]) {
            expect(params.get(key)).toBeNull()
        }
        expect(params.get("view")).toBe("all")
        expect(params.get("sort")).toBe("identity")
        expect(params.get("dir")).toBe("desc")
        expect(params.get("returnTo")).toBe("/commerce/consumption-orders")
    })

    it("resetMoreFilters clears structured conditions but keeps q and quick filters, panel stays open", () => {
        navigation.searchParams = new URLSearchParams(
            "q=SFO&aftersalePending=1&supplierId=sup_1&fulfillmentStatus=EXCEPTION&cancelStatus=FAILED&refundStatus=MANUAL&paidFrom=2026-08-01&paidTo=2026-08-08&view=all",
        )
        const { result } = renderFilters()

        act(() => {
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.resetMoreFilters()
        })

        const params = urlOf(navigation.replace.mock.calls[0]!)
        expect(params.get("q")).toBe("SFO")
        expect(params.get("aftersalePending")).toBe("1")
        expect(params.get("view")).toBe("all")
        expect(params.get("supplierId")).toBeNull()
        expect(params.get("fulfillmentStatus")).toBeNull()
        expect(params.get("cancelStatus")).toBeNull()
        expect(params.get("refundStatus")).toBeNull()
        expect(params.get("paidFrom")).toBeNull()
        expect(params.get("paidTo")).toBeNull()
        expect(params.get("page")).toBeNull()
        expect(result.current.panelOpen).toBe(true)
    })

    it("removes a single applied condition by key", () => {
        navigation.searchParams = new URLSearchParams("q=SFO&page=2")
        const { result } = renderFilters()
        act(() => {
            result.current.removeFilter("q")
        })
        let params = urlOf(navigation.replace.mock.calls[0]!)
        expect(params.get("q")).toBeNull()
        expect(params.get("page")).toBeNull()
        expect(result.current.searchDraft).toBe("")

        navigation.searchParams = new URLSearchParams(
            "paidFrom=2026-08-01&paidTo=2026-08-08&supplierId=sup_1",
        )
        const second = renderFilters()
        act(() => {
            second.result.current.removeFilter("paidRange")
        })
        params = urlOf(navigation.replace.mock.calls[1]!)
        expect(params.get("paidFrom")).toBeNull()
        expect(params.get("paidTo")).toBeNull()

        navigation.searchParams = new URLSearchParams("aftersalePending=1")
        const third = renderFilters()
        act(() => {
            third.result.current.removeFilter("aftersalePending")
        })
        params = urlOf(navigation.replace.mock.calls[2]!)
        expect(params.get("aftersalePending")).toBeNull()
    })
})

describe("useSupplierOrdersFilters — chips and backfill", () => {
    it("derives one removable chip per applied condition", () => {
        navigation.searchParams = new URLSearchParams(
            "q=SFO&supplierId=sup_1&fulfillmentStatus=EXCEPTION&cancelStatus=FAILED&refundStatus=MANUAL&aftersalePending=1&paidFrom=2026-08-01&paidTo=2026-08-08",
        )
        const { result } = renderFilters()

        expect(result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：SFO" },
            { key: "supplierId", label: "供应商：sup_1" },
            { key: "fulfillmentStatuses", label: "履约状态：异常" },
            { key: "cancelStatuses", label: "取消状态：失败" },
            { key: "refundStatuses", label: "退款状态：待人工" },
            { key: "aftersalePending", label: "售后待处理" },
            {
                key: "paidRange",
                label: "支付时间：2026-08-01 至 2026-08-08",
            },
        ])
        expect(result.current.hasActiveFilters).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
    })

    it("backfills drafts from URL changes without touching the panel state", () => {
        navigation.searchParams = new URLSearchParams("supplierId=sup_1")
        const { result, rerender } = renderFilters()
        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.setSupplierIdDraft("junk")
            result.current.setFulfillmentStatusesDraft(["EXCEPTION"])
            result.current.setPanelOpen(false)
        })

        navigation.searchParams = new URLSearchParams(
            "supplierId=sup_2&fulfillmentStatus=COMPLETED&q=NEW",
        )
        rerender()

        expect(result.current.supplierIdDraft).toBe("sup_2")
        expect(result.current.fulfillmentStatusesDraft).toEqual(["COMPLETED"])
        expect(result.current.searchDraft).toBe("NEW")
        expect(result.current.panelOpen).toBe(false)
    })

    it("has no active filters or chips when the URL is empty", () => {
        const { result } = renderFilters()

        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.appliedChips).toEqual([])
    })
})
