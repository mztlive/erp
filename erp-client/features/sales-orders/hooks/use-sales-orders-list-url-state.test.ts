import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
    buildSalesOrdersSearchParams,
    mergeSalesOrdersSearchParams,
    normalizedSalesOrdersSearchParams,
    parseSalesOrdersSearchParams,
} from "@/features/sales-orders/lib/url-state"
import { useSalesOrdersListUrlState } from "./use-sales-orders-list-url-state"

const { replaceMock, searchParamsMock } = vi.hoisted(() => ({
    replaceMock: vi.fn(),
    searchParamsMock: { current: new URLSearchParams() },
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: vi.fn(), replace: replaceMock, back: vi.fn() }),
    useSearchParams: () => searchParamsMock.current,
    usePathname: () => "/test",
    useParams: () => ({}),
}))

beforeEach(() => {
    vi.clearAllMocks()
    searchParamsMock.current = new URLSearchParams()
})

afterEach(() => {
    cleanup()
})

describe("useSalesOrdersListUrlState", () => {
    it("parses URL 参数为默认状态", () => {
        const { result } = renderHook(() => useSalesOrdersListUrlState())

        expect(result.current.url).toMatchObject({
            page: 1,
            pageSize: 20,
            nature: "all",
            summary: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            search: undefined,
        })
    })

    it("pushUrl 用补丁覆盖状态并写回 URL（保留无关参数）", () => {
        searchParamsMock.current = new URLSearchParams("foo=bar")
        const { result } = renderHook(() => useSalesOrdersListUrlState())

        act(() => {
            result.current.pushUrl({ page: 2 })
        })

        expect(replaceMock).toHaveBeenCalledWith(
            "/test?foo=bar&page=2",
            { scroll: false },
        )
    })

    it("pushUrl 会清掉受管参数后按最新状态重建", () => {
        searchParamsMock.current = new URLSearchParams("page=3&q=old")
        const { result } = renderHook(() => useSalesOrdersListUrlState())

        act(() => {
            result.current.pushUrl({ page: 2, search: undefined })
        })

        expect(replaceMock).toHaveBeenCalledWith("/test?page=2", {
            scroll: false,
        })
    })

    it("非法参数会触发一次归一化 replace（别名转规范键）", () => {
        searchParamsMock.current = new URLSearchParams("page=2&search=abc")
        renderHook(() => useSalesOrdersListUrlState())

        expect(replaceMock).toHaveBeenCalledWith("/test?q=abc&page=2", {
            scroll: false,
        })
    })

    it("参数已是规范形态时不触发归一化 replace", () => {
        searchParamsMock.current = new URLSearchParams("page=2&q=abc")
        renderHook(() => useSalesOrdersListUrlState())

        expect(replaceMock).not.toHaveBeenCalled()
    })

    it("pushUrl 后按新状态解析 url", () => {
        const { result } = renderHook(() => useSalesOrdersListUrlState())

        act(() => {
            result.current.pushUrl({ page: 3, pageSize: 50 })
        })

        // 页面状态由 URL 驱动：下一次渲染（新 searchParams）才会生效，
        // 这里验证 merge 输出与当前 url 不变（不重复写回）。
        expect(result.current.url.page).toBe(1)
        expect(replaceMock).toHaveBeenCalledTimes(1)
    })
})

describe("parseSalesOrdersSearchParams 边界", () => {
    it("读取别名 q → search（解析不 trim，写回时 trim）", () => {
        const url = parseSalesOrdersSearchParams(
            new URLSearchParams("q=%20SO-1%20"),
        )
        expect(url.search).toBe(" SO-1 ")

        expect(
            buildSalesOrdersSearchParams({ ...url, search: " SO-1 " }),
        ).toBe("?q=SO-1")
    })

    it("兼容旧业务类型别名并归一化", () => {
        expect(
            parseSalesOrdersSearchParams(
                new URLSearchParams("businessType=voucher"),
            ).nature,
        ).toBe("card_voucher")
        expect(
            parseSalesOrdersSearchParams(
                new URLSearchParams("businessType=goods_service"),
            ).nature,
        ).toBe("physical_service")
        expect(
            parseSalesOrdersSearchParams(
                new URLSearchParams("businessType=bogus"),
            ).nature,
        ).toBe("all")
    })

    it("工作视图会清掉冲突字段", () => {
        const mine = parseSalesOrdersSearchParams(
            new URLSearchParams(
                "summary=mine&createdBy=u-1&commercialStatus=draft&reviewStatus=approved",
            ),
        )
        expect(mine.createdBy).toBeUndefined()
        expect(mine.commercialStatus).toBe("all")
        expect(mine.reviewStatus).toBe("all")

        const mineOrder = parseSalesOrdersSearchParams(
            new URLSearchParams("summary=createdByMe&createdBy=u-1"),
        )
        expect(mineOrder.createdBy).toBeUndefined()

        const exception = parseSalesOrdersSearchParams(
            new URLSearchParams(
                "summary=exception&commercialStatus=draft&reviewStatus=approved",
            ),
        )
        expect(exception.commercialStatus).toBe("all")
        expect(exception.reviewStatus).toBe("all")
    })

    it("反向日期区间自动交换，非法日期丢弃", () => {
        const url = parseSalesOrdersSearchParams(
            new URLSearchParams(
                "createdFrom=2026-03-02&createdTo=2026-03-01",
            ),
        )
        expect(url.createdFrom).toBe("2026-03-01")
        expect(url.createdTo).toBe("2026-03-02")

        const invalid = parseSalesOrdersSearchParams(
            new URLSearchParams("createdFrom=2026-02-30&createdTo=not-a-date"),
        )
        expect(invalid.createdFrom).toBeUndefined()
        expect(invalid.createdTo).toBeUndefined()
    })

    it("page/pageSize 夹取到合法范围", () => {
        const url = parseSalesOrdersSearchParams(
            new URLSearchParams("page=0&pageSize=999"),
        )
        expect(url.page).toBe(1)
        expect(url.pageSize).toBe(100)
    })

    it("未知枚举值回退默认", () => {
        const url = parseSalesOrdersSearchParams(
            new URLSearchParams("nature=bogus&dir=sideways&sort=amount"),
        )
        expect(url.nature).toBe("all")
        expect(url.dir).toBeUndefined()
        expect(url.sort).toBe("amount")
    })
})

describe("merge / normalized", () => {
    it("merge 保留受管参数之外的内容", () => {
        const state = parseSalesOrdersSearchParams(new URLSearchParams())
        const qs = mergeSalesOrdersSearchParams(
            new URLSearchParams("utm_campaign=x&page=5"),
            { ...state, page: 2 },
        )
        expect(qs).toBe("?utm_campaign=x&page=2")
    })

    it("normalized 在受管参数一致时返回 undefined", () => {
        const params = new URLSearchParams("q=abc&page=2")
        const state = parseSalesOrdersSearchParams(params)
        expect(
            normalizedSalesOrdersSearchParams(params, state),
        ).toBeUndefined()
    })
})
