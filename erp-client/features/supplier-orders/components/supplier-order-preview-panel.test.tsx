import { describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"

vi.mock("next/link", () => ({
    default: (props: {
        href?: string
        className?: string
        children?: ReactNode
    }) => <a {...props}>{props.children}</a>,
}))

import { seedError } from "./supplier-order-preview-panel"
import { makeDetail } from "@/features/supplier-orders/hooks/use-supplier-order-center-fixtures"

describe("seedError", () => {
    it("prefers the order error summary", () => {
        const order = makeDetail()
        order.order.errorSummary = "供应商同步失败"
        expect(seedError(order)).toBe("供应商同步失败")
    })

    it("falls back to the first blocker message for unknown results", () => {
        const order = makeDetail({
            actionBlockers: [
                {
                    action: "QUERY_RESULT",
                    code: "NO_QUERY_CAPABILITY",
                    message: "该供应商无查询能力",
                },
            ],
        })
        expect(seedError(order)).toBe("该供应商无查询能力")
    })

    it("explains rejections without extra blockers", () => {
        const order = makeDetail()
        order.order.fulfillmentStatus = "REJECTED"
        expect(seedError(order)).toBe(
            "供应商明确拒单。支付与成本记录保留，不自动重试。",
        )
    })

    it("explains exceptions without extra blockers", () => {
        const order = makeDetail()
        order.order.fulfillmentStatus = "EXCEPTION"
        expect(seedError(order)).toBe(
            "履约异常。支付与消费记录不删除，请按售后或转人工处理。",
        )
    })

    it("provides a default explanation when nothing is reported", () => {
        const order = makeDetail()
        order.order.fulfillmentStatus = "SHIPPED"
        expect(seedError(order)).toBe("无额外异常说明。")
    })
})
