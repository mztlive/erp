import { describe, expect, test } from "vitest"

import {
    countScopeIds,
    isScopeChangedError,
    parseScopeIdList,
} from "@/lib/funds-scope"
import { invoiceRequestScopeKeys } from "@/features/invoice-requests/hooks/scoped-queries"
import { receivableScopeKeys } from "@/features/customer-receivables/hooks/scoped-queries"
import { supplierScopeKeys } from "@/features/supplier-payables/hooks/scoped-queries"

describe("资金范围共享解析", () => {
    test("空值返回 undefined，有值原样返回", () => {
        expect(parseScopeIdList(null)).toBeUndefined()
        expect(parseScopeIdList("  ")).toBeUndefined()
        expect(parseScopeIdList("user-1,user-2")).toBe("user-1,user-2")
    })

    test("已选 ID 计数忽略空片段", () => {
        expect(countScopeIds(undefined)).toBe(0)
        expect(countScopeIds("user-1,,user-2")).toBe(2)
    })

    test("范围版本冲突可识别", () => {
        expect(isScopeChangedError({ code: "DATA_SCOPE_CHANGED" })).toBe(true)
        expect(
            isScopeChangedError(new Error("DATA_SCOPE_CHANGED 请重试")),
        ).toBe(true)
        expect(isScopeChangedError(new Error("网络失败"))).toBe(false)
    })
})

describe("三功能 Query key 隔离", () => {
    test("M07/M08/M09 使用不同根键", () => {
        expect(receivableScopeKeys.all).toEqual([
            "customer-receivables",
            "scoped",
        ])
        expect(invoiceRequestScopeKeys.all).toEqual([
            "invoice-requests",
            "scoped",
        ])
        expect(supplierScopeKeys.all).toEqual(["supplier-payables", "scoped"])
    })

    test("有效条件进入 Query key", () => {
        const key = receivableScopeKeys.list({
            view: "receipt",
            page: 2,
            pageSize: 20,
            salesOwnerUserIds: "user-1",
            operatorUserIds: "user-2",
            operatorKind: "settle",
            scopeVersion: "v1",
        })
        expect(JSON.stringify(key)).toContain("user-1")
        expect(JSON.stringify(key)).toContain("settle")
        expect(JSON.stringify(key)).toContain("v1")
    })

    test("供应商范围版本进入 Query key", () => {
        const key = supplierScopeKeys.list({
            view: "payment",
            page: 2,
            pageSize: 20,
            scopeVersion: "v9",
        })
        expect(JSON.stringify(key)).toContain("v9")
    })
})
