import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, renderHook, waitFor } from "@testing-library/react"

import {
    buildSupplierOfferingAppliedChips,
    useSupplierOfferingsPageState,
} from "./use-supplier-offerings-page-state"
import { parseSupplierOfferingsSearchParams } from "@/features/supplier-offerings/lib/url-state"

const { useRouter, useSearchParams, usePathname, replaceSpy } = vi.hoisted(
    () => {
        const replaceSpy = vi.fn()
        return {
            useRouter: vi.fn(() => ({
                push: vi.fn(),
                replace: replaceSpy,
                back: vi.fn(),
            })),
            useSearchParams: vi.fn(() => new URLSearchParams()),
            usePathname: vi.fn(() => "/test"),
            replaceSpy,
        }
    },
)

vi.mock("next/navigation", () => ({
    useRouter,
    useSearchParams,
    usePathname,
    useParams: () => ({}),
}))

beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ""
    useSearchParams.mockReturnValue(new URLSearchParams())
    usePathname.mockReturnValue("/test")
})

afterEach(() => {
    cleanup()
})

describe("useSupplierOfferingsPageState", () => {
    it("derives defaults from an empty URL", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.urlState.page).toBe(1)
        expect(result.current.urlState.q).toBeUndefined()
        expect(result.current.urlState.skuId).toBeUndefined()
        expect(result.current.searchDraft).toBe("")
        expect(result.current.skuIdDraft).toBeNull()
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.sourceTypeDraft).toBe("all")
        expect(result.current.availabilityStatusDraft).toBe("all")
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.skuLocked).toBe(false)
        expect(result.current.taskMode).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.hasFilters).toBe(false)
        expect(result.current.appliedFilterLabels).toEqual([])
    })

    it("parses query params into applied state and initial drafts", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=ACTIVE&page=3&sku_no=SKU-001"),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.urlState.q).toBe("abc")
        expect(result.current.urlState.status).toBe("ACTIVE")
        expect(result.current.urlState.page).toBe(3)
        expect(result.current.urlState.skuNo).toBe("SKU-001")
        expect(result.current.searchDraft).toBe("abc")
        expect(result.current.statusDraft).toBe("ACTIVE")
        expect(result.current.hasFilters).toBe(true)
    })

    it("ignores invalid enum values and clamps page to at least 1", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("status=BOGUS&page=0&sourceType=API"),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.urlState.status).toBeUndefined()
        expect(result.current.urlState.page).toBe(1)
        expect(result.current.urlState.sourceType).toBe("API")
    })

    it("locks the SKU only when both skuId and returnTo are present", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("skuId=sku_1&returnTo=/products"),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.skuLocked).toBe(true)
        // 锁定条件不计入结构化筛选与筛选标签。
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.hasFilters).toBe(true)
        expect(result.current.appliedFilterLabels).toEqual([])

        useSearchParams.mockReturnValue(new URLSearchParams("skuId=sku_1"))
        const { result: unlocked } = renderHook(() =>
            useSupplierOfferingsPageState(),
        )
        expect(unlocked.current.skuLocked).toBe(false)
        expect(unlocked.current.hasStructuredFilters).toBe(true)
        expect(unlocked.current.appliedFilterLabels).toEqual([
            "已选择公司 SKU",
        ])
    })

    it("derives taskMode from workItemId", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("workItemId=wi_1&queueContextId=qc_1"),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.taskMode).toBe(true)
        expect(result.current.urlState.workItemId).toBe("wi_1")
        expect(result.current.urlState.queueContextId).toBe("qc_1")
    })

    it("builds applied filter labels in field order", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams(
                "q=abc&supplierId=sup_1&status=ACTIVE&availabilityStatus=STALE",
            ),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.appliedFilterLabels).toEqual([
            "订货编码包含“abc”",
            "已选择供应商",
            "关系状态：启用",
            "当前可供：数据已过期",
        ])
    })

    it("applyFilters writes drafts into the URL and resets the page", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.setSearchDraft("  abc  ")
            result.current.setStatusDraft("PAUSED")
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(replaceSpy).toHaveBeenCalledTimes(1)
        expect(replaceSpy).toHaveBeenCalledWith(
            "/test?q=abc&status=PAUSED",
            { scroll: false },
        )
    })

    it("applyFilters omits drafts that are empty or set to 'all'", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.setSearchDraft("   ")
            result.current.setSourceTypeDraft("all")
            result.current.applyFilters()
        })

        expect(replaceSpy).toHaveBeenCalledWith("/test", { scroll: false })
    })

    it("clearFilters resets drafts and strips all filter params", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=PAUSED"),
        )
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.setSearchDraft("abc")
            result.current.setStatusDraft("PAUSED")
            result.current.clearFilters()
        })

        expect(result.current.searchDraft).toBe("")
        expect(result.current.skuIdDraft).toBeNull()
        expect(result.current.skuNoDraft).toBe("")
        expect(result.current.productNoDraft).toBe("")
        expect(result.current.supplierIdDraft).toBeNull()
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.sourceTypeDraft).toBe("all")
        expect(result.current.availabilityStatusDraft).toBe("all")
        expect(result.current.filterPanelOpen).toBe(false)
        expect(replaceSpy).toHaveBeenCalledWith("/test", { scroll: false })
    })

    it("clearSkuLock removes only the locked skuId and keeps navigation context", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("skuId=sku_1&returnTo=/products"),
        )
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.clearSkuLock()
        })

        expect(result.current.skuIdDraft).toBeNull()
        expect(replaceSpy).toHaveBeenCalledWith(
            "/test?returnTo=%2Fproducts",
            { scroll: false },
        )
    })

    it("backfills drafts when the URL changes via back/forward", async () => {
        const { result, rerender } = renderHook(() =>
            useSupplierOfferingsPageState(),
        )

        useSearchParams.mockReturnValue(
            new URLSearchParams("q=xyz&status=PAUSED"),
        )
        rerender()

        await waitFor(() => {
            expect(result.current.searchDraft).toBe("xyz")
            expect(result.current.statusDraft).toBe("PAUSED")
        })
        // URL 回填只同步草稿，不得抢夺面板展开态（§5.5）。
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("opens the panel on first mount when the deep link has structured filters", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("status=ACTIVE&supplierId=sup_1"),
        )

        const { result } = renderHook(() => useSupplierOfferingsPageState())

        expect(result.current.filterPanelOpen).toBe(true)
    })

    it("applyFilters collapses the panel after writing the URL", () => {
        useSearchParams.mockReturnValue(new URLSearchParams("status=ACTIVE"))
        const { result } = renderHook(() => useSupplierOfferingsPageState())
        expect(result.current.filterPanelOpen).toBe(true)

        act(() => {
            result.current.applyFilters()
        })

        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("resetMoreFilters keeps the keyword and the product-page SKU lock while clearing structured drafts", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams(
                "q=abc&skuId=sku_1&returnTo=/products&status=PAUSED&sku_no=SKU-001",
            ),
        )
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.resetMoreFilters()
        })

        expect(result.current.searchDraft).toBe("abc")
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.skuNoDraft).toBe("")
        expect(result.current.skuIdDraft).toBe("sku_1")
        // 面板保持展开（§5.6）。
        expect(result.current.filterPanelOpen).toBe(true)
        expect(replaceSpy).toHaveBeenCalledWith(
            "/test?q=abc&skuId=sku_1&returnTo=%2Fproducts",
            { scroll: false },
        )
    })

    it("resetMoreFilters clears a panel-chosen SKU when it is not locked", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&skuId=sku_1"),
        )
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.resetMoreFilters()
        })

        expect(result.current.skuIdDraft).toBeNull()
        expect(replaceSpy).toHaveBeenCalledWith("/test?q=abc", {
            scroll: false,
        })
    })

    it("clearFilters keeps the product-page SKU lock while clearing the rest", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams(
                "q=abc&status=PAUSED&skuId=sku_1&returnTo=/products",
            ),
        )
        const { result } = renderHook(() => useSupplierOfferingsPageState())

        act(() => {
            result.current.clearFilters()
        })

        expect(result.current.searchDraft).toBe("")
        expect(result.current.statusDraft).toBe("all")
        expect(result.current.skuIdDraft).toBe("sku_1")
        expect(result.current.filterPanelOpen).toBe(false)
        expect(replaceSpy).toHaveBeenCalledWith(
            "/test?skuId=sku_1&returnTo=%2Fproducts",
            { scroll: false },
        )
    })

    it("removeFilter removes only the given applied condition", () => {
        useSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=PAUSED&sourceType=API"),
        )
        const { result, rerender } = renderHook(() =>
            useSupplierOfferingsPageState(),
        )

        act(() => {
            result.current.removeFilter("status")
        })
        expect(replaceSpy).toHaveBeenCalledWith("/test?q=abc&sourceType=API", {
            scroll: false,
        })

        // 模拟 router.replace 后的 URL 回填：Applied 只读 URL（§5.1）
        useSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&sourceType=API"),
        )
        rerender()

        act(() => {
            result.current.removeFilter("q")
        })
        expect(result.current.searchDraft).toBe("")
        expect(replaceSpy).toHaveBeenCalledWith("/test?sourceType=API", {
            scroll: false,
        })
    })

    it("does not overwrite the keyword draft while the search input is focused", () => {
        const { result, rerender } = renderHook(() =>
            useSupplierOfferingsPageState(),
        )

        act(() => {
            result.current.setSearchDraft("abc")
        })
        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            result.current.searchInputRef.current = input
            input.focus()
        })

        useSearchParams.mockReturnValue(new URLSearchParams("q=xyz"))
        rerender()

        expect(document.activeElement).toBe(input)
        expect(result.current.searchDraft).toBe("abc")
    })

    it("focuses the search input on / when no dialog or sheet is open", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())
        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            result.current.searchInputRef.current = input
        })

        const event = new KeyboardEvent("keydown", {
            key: "/",
            cancelable: true,
        })
        act(() => {
            window.dispatchEvent(event)
        })

        expect(document.activeElement).toBe(input)
        expect(event.defaultPrevented).toBe(true)
    })

    it("ignores / while a dialog or sheet is open", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())
        const input = document.createElement("input")
        document.body.appendChild(input)
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        act(() => {
            result.current.searchInputRef.current = input
        })

        const event = new KeyboardEvent("keydown", {
            key: "/",
            cancelable: true,
        })
        act(() => {
            window.dispatchEvent(event)
        })

        expect(document.activeElement).not.toBe(input)
        expect(event.defaultPrevented).toBe(false)
    })

    it("ignores / typed inside inputs and other keys entirely", () => {
        const { result } = renderHook(() => useSupplierOfferingsPageState())
        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            result.current.searchInputRef.current = input
        })

        const fromInput = new KeyboardEvent("keydown", {
            key: "/",
            bubbles: true,
            cancelable: true,
        })
        act(() => {
            input.dispatchEvent(fromInput)
        })
        expect(fromInput.defaultPrevented).toBe(false)

        const otherKey = new KeyboardEvent("keydown", {
            key: "a",
            cancelable: true,
        })
        act(() => {
            window.dispatchEvent(otherKey)
        })
        expect(otherKey.defaultPrevented).toBe(false)
        expect(document.activeElement).not.toBe(input)
    })

    it("unregisters the shortcut listener on unmount", async () => {
        const { result, unmount } = renderHook(() =>
            useSupplierOfferingsPageState(),
        )
        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            result.current.searchInputRef.current = input
        })
        act(() => {
            unmount()
        })
        // React 19 在 unmount 后的微任务中执行 passive effect 清理。
        await act(async () => {
            await Promise.resolve()
        })

        const event = new KeyboardEvent("keydown", {
            key: "/",
            cancelable: true,
        })
        window.dispatchEvent(event)

        expect(event.defaultPrevented).toBe(false)
        expect(document.activeElement).not.toBe(input)
    })

})

describe("buildSupplierOfferingAppliedChips", () => {
    it("derives a removable chip for every applied condition in field order", () => {
        const urlState = parseSupplierOfferingsSearchParams(
            new URLSearchParams(
                "q=abc&skuId=sku_1&sku_no=SKU-001&product_no=P-1001&supplierId=sup_1&status=ACTIVE&sourceType=EXCEL&availabilityStatus=STALE",
            ),
        )
        const chips = buildSupplierOfferingAppliedChips(urlState, {
            skuNoLabel: "SKU-001",
            supplierNameLabel: "华东供应商",
        })

        expect(chips.map((chip) => chip.key)).toEqual([
            "q",
            "skuId",
            "skuNo",
            "productNo",
            "supplierId",
            "status",
            "sourceType",
            "availabilityStatus",
        ])
        expect(chips.map((chip) => chip.label)).toEqual([
            "搜索：abc",
            "公司 SKU：SKU-001",
            "SKU 编号：SKU-001",
            "SPU 编号：P-1001",
            "供应商：华东供应商",
            "关系状态：启用",
            "登记来源：Excel",
            "当前可供：数据已过期",
        ])
    })

    it("falls back to a generic label when the business label is unknown", () => {
        const urlState = parseSupplierOfferingsSearchParams(
            new URLSearchParams("skuId=sku_1&supplierId=sup_1"),
        )
        const chips = buildSupplierOfferingAppliedChips(urlState, {})

        expect(chips.map((chip) => chip.label)).toEqual([
            "公司 SKU：已选择",
            "供应商：已选择",
        ])
    })

    it("returns no chips for the default state", () => {
        expect(
            buildSupplierOfferingAppliedChips(
                parseSupplierOfferingsSearchParams(new URLSearchParams()),
                {},
            ),
        ).toEqual([])
    })
})
