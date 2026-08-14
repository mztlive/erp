import { describe, it, expect } from "vitest"

import {
    buildPurchaseOrdersSearchParams,
    parsePurchaseOrdersSearchParams,
    type PurchaseOrdersUrlState,
} from "./url-state"

const BASE_STATE: PurchaseOrdersUrlState = {
    q: undefined,
    status: "all",
    metric: "all",
    page: 1,
    pageSize: 20,
    sort: undefined,
    basisId: undefined,
}

describe("parsePurchaseOrdersSearchParams", () => {
    it("回退到默认值（空参数）", () => {
        expect(parsePurchaseOrdersSearchParams(new URLSearchParams())).toEqual(
            BASE_STATE,
        )
    })

    it("解析全部合法参数", () => {
        const params = new URLSearchParams(
            "q=abc&status=DRAFT&metric=review&page=3&pageSize=50&sort=amount:desc&basisId=bas_1",
        )
        expect(parsePurchaseOrdersSearchParams(params)).toEqual({
            q: "abc",
            status: "DRAFT",
            metric: "review",
            page: 3,
            pageSize: 50,
            sort: "amount:desc",
            basisId: "bas_1",
        })
    })

    it("非法枚举回退默认值", () => {
        const params = new URLSearchParams("status=POSTED&metric=nope")
        const state = parsePurchaseOrdersSearchParams(params)
        expect(state.status).toBe("all")
        expect(state.metric).toBe("all")
    })

    it("页码非法值回退默认", () => {
        expect(
            parsePurchaseOrdersSearchParams(new URLSearchParams("page=0"))
                .page,
        ).toBe(1)
        expect(
            parsePurchaseOrdersSearchParams(new URLSearchParams("page=abc"))
                .page,
        ).toBe(1)
        expect(
            parsePurchaseOrdersSearchParams(new URLSearchParams("page=-2"))
                .page,
        ).toBe(1)
    })

    it("pageSize 超上限截断到 100", () => {
        expect(
            parsePurchaseOrdersSearchParams(
                new URLSearchParams("pageSize=500"),
            ).pageSize,
        ).toBe(100)
    })

    it("状态枚举包含全部状态过滤值", () => {
        for (const status of [
            "all",
            "DRAFT",
            "PENDING_REVIEW",
            "EFFECTIVE",
            "PARTIAL",
            "COMPLETED",
            "VOID",
        ]) {
            const state = parsePurchaseOrdersSearchParams(
                new URLSearchParams(`status=${status}`),
            )
            expect(state.status).toBe(status)
        }
    })
})

describe("buildPurchaseOrdersSearchParams", () => {
    it("全默认状态输出空串", () => {
        expect(buildPurchaseOrdersSearchParams(BASE_STATE)).toBe("")
    })

    it("仅写入非默认参数", () => {
        expect(
            buildPurchaseOrdersSearchParams({
                ...BASE_STATE,
                status: "DRAFT",
                page: 3,
            }),
        ).toBe("?status=DRAFT&page=3")
    })

    it("搜索词写入前 trim，空词不写入", () => {
        expect(
            buildPurchaseOrdersSearchParams({ ...BASE_STATE, q: " abc " }),
        ).toBe("?q=abc")
        expect(
            buildPurchaseOrdersSearchParams({ ...BASE_STATE, q: "   " }),
        ).toBe("")
    })

    it("page 回默认 1 时不写入", () => {
        expect(
            buildPurchaseOrdersSearchParams({ ...BASE_STATE, page: 1 }),
        ).toBe("")
    })

    it("build 与 parse 往返一致", () => {
        const state: PurchaseOrdersUrlState = {
            q: "钢",
            status: "EFFECTIVE",
            metric: "fulfill",
            page: 4,
            pageSize: 50,
            sort: "document:asc",
            basisId: "bas_9",
        }
        const built = buildPurchaseOrdersSearchParams(state)
        expect(parsePurchaseOrdersSearchParams(new URLSearchParams(built))).toEqual(
            state,
        )
    })
})
