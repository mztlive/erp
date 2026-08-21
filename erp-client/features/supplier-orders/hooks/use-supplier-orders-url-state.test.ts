import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

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

import { useSupplierOrdersUrlState } from "./use-supplier-orders-url-state"

function renderUrlState() {
    return renderHook(() => useSupplierOrdersUrlState())
}

beforeEach(() => {
    navigation.push.mockReset()
    navigation.replace.mockReset()
    navigation.pathname = "/supplier-api/orders"
    navigation.searchParams = new URLSearchParams()
})

describe("useSupplierOrdersUrlState — parsing", () => {
    it("parses the current search params with codec defaults", () => {
        navigation.searchParams = new URLSearchParams("view=all&page=3&q=A-1")
        const { result } = renderUrlState()

        expect(result.current.url.view).toBe("all")
        expect(result.current.url.page).toBe(3)
        expect(result.current.url.q).toBe("A-1")
        expect(result.current.returnTo).toBeUndefined()
    })

    it("re-parses when the search params change", () => {
        navigation.searchParams = new URLSearchParams("view=all")
        const { result, rerender } = renderUrlState()

        navigation.searchParams = new URLSearchParams("view=recent_completed")
        rerender()

        expect(result.current.url.view).toBe("recent_completed")
    })
})

describe("useSupplierOrdersUrlState — updateUrl", () => {
    it("replaces by default with the built query string and no scroll", () => {
        navigation.searchParams = new URLSearchParams("view=all&page=1")
        const { result } = renderUrlState()

        act(() => {
            result.current.updateUrl({ page: 2, q: "SFO" })
        })

        expect(navigation.replace).toHaveBeenCalledWith(
            expect.stringMatching(/^\/supplier-api\/orders\?/),
            { scroll: false },
        )
        const url = navigation.replace.mock.calls[0]![0]
        const params = new URLSearchParams(url.slice(url.indexOf("?") + 1))
        expect(params.get("page")).toBe("2")
        expect(params.get("q")).toBe("SFO")
        expect(navigation.push).not.toHaveBeenCalled()
    })

    it("pushes for detail navigation", () => {
        const { result } = renderUrlState()

        act(() => {
            result.current.updateUrl({ preview: "so_1" }, "push")
        })

        expect(navigation.push).toHaveBeenCalledWith(
            "/supplier-api/orders?preview=so_1",
            { scroll: false },
        )
    })

    it("omits default values (aftersalePending=false, page=1) from the URL", () => {
        navigation.searchParams = new URLSearchParams("view=all&page=3")
        const { result } = renderUrlState()

        act(() => {
            result.current.updateUrl({ page: 4 })
        })

        const url = navigation.replace.mock.calls[0]![0]
        const params = new URLSearchParams(url.slice(url.indexOf("?") + 1))
        expect(params.get("page")).toBe("4")
        expect(params.get("aftersalePending")).toBeNull()
    })

    it("drops the default aftersalePending=false when toggled off", () => {
        navigation.searchParams = new URLSearchParams("aftersalePending=1")
        const { result } = renderUrlState()

        act(() => {
            result.current.updateUrl({ aftersalePending: false, page: 1 })
        })

        const url = navigation.replace.mock.calls[0]![0]
        const params = new URLSearchParams(url.slice(url.indexOf("?") + 1))
        expect(params.get("aftersalePending")).toBeNull()
    })

    it("preserves the returnTo context in every navigation", () => {
        navigation.searchParams = new URLSearchParams(
            "page=1&returnTo=%2Fcommerce%2Fconsumption-orders",
        )
        const { result } = renderUrlState()

        act(() => {
            result.current.updateUrl({ page: 2 })
        })

        const url = navigation.replace.mock.calls[0]![0]
        expect(url).toContain("returnTo=%2Fcommerce%2Fconsumption-orders")
    })
    it("degrades invalid enum values to defaults on parse", () => {
        navigation.searchParams = new URLSearchParams(
            "view=bogus&dir=sideways&fulfillmentStatus=GARBAGE,EXCEPTION",
        )
        const { result } = renderUrlState()

        expect(result.current.url.view).toBe("actionable")
        expect(result.current.url.dir).toBeUndefined()
        expect(result.current.url.fulfillmentStatuses).toEqual(["EXCEPTION"])
    })
})

describe("useSupplierOrdersUrlState — W25 drill-down", () => {
    it("redirects to the order center for W25 deep links", () => {
        navigation.searchParams = new URLSearchParams(
            "supplierOrderId=so_1&from=W25&mallOrderId=mo_1",
        )
        renderUrlState()

        expect(navigation.replace).toHaveBeenCalledWith(
            "/supplier-api/orders/so_1?from=mall-order&sourceId=mo_1",
        )
    })

    it("redirects when openCenter=1 with the preview alias", () => {
        navigation.searchParams = new URLSearchParams(
            "preview=so_2&openCenter=1",
        )
        renderUrlState()

        expect(navigation.replace).toHaveBeenCalledWith(
            "/supplier-api/orders/so_2",
        )
    })

    it("stays on the list without a drill-down trigger", () => {
        navigation.searchParams = new URLSearchParams("preview=so_3&from=other")
        renderUrlState()

        expect(navigation.replace).not.toHaveBeenCalled()
    })
})
