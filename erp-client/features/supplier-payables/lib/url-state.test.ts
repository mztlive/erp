import { describe, expect, test } from "vitest"

import { parsePreviewKind, parseView, patchForViewChange } from "./url-state"

describe("parseView", () => {
    test("识别四个工作视图", () => {
        expect(parseView("payable")).toBe("payable")
        expect(parseView("payment")).toBe("payment")
        expect(parseView("purchase_invoice")).toBe("purchase_invoice")
        expect(parseView("unallocated")).toBe("unallocated")
    })

    test("缺省或未知值回退应付台账", () => {
        expect(parseView(null)).toBe("payable")
        expect(parseView("")).toBe("payable")
        expect(parseView("receipt")).toBe("payable")
    })
})

describe("patchForViewChange", () => {
    test("切到付款时清应付筛选、轨道和分页", () => {
        expect(patchForViewChange("payment")).toEqual({
            view: "payment",
            page: null,
            supplierId: null,
            sourceType: null,
            status: null,
            due: null,
            paymentGate: null,
            track: null,
        })
    })

    test("切到进项发票时同样清应付筛选和轨道", () => {
        expect(patchForViewChange("purchase_invoice")).toEqual({
            view: "purchase_invoice",
            page: null,
            supplierId: null,
            sourceType: null,
            status: null,
            due: null,
            paymentGate: null,
            track: null,
        })
    })

    test("切到应付台账时保留应付筛选、清轨道", () => {
        expect(patchForViewChange("payable")).toEqual({
            view: "payable",
            page: null,
            track: null,
        })
    })

    test("切到待核销时清应付筛选、保留轨道", () => {
        expect(patchForViewChange("unallocated")).toEqual({
            view: "unallocated",
            page: null,
            supplierId: null,
            sourceType: null,
            status: null,
            due: null,
            paymentGate: null,
        })
    })
})

test("预览 URL 识别进项发票与待核销记录", () => {
    expect(parsePreviewKind("invoice")).toBe("invoice")
    expect(parsePreviewKind("unallocated")).toBe("unallocated")
    expect(parsePreviewKind("unknown")).toBe("payable")
})
