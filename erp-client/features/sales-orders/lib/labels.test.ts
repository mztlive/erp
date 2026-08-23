import { describe, expect, it } from "vitest"

import {
    isPendingReviewStage,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

describe("sales order stage labels", () => {
    it("treats unified approval as an active review stage", () => {
        expect(isPendingReviewStage("in_approval")).toBe(true)
        expect(isPendingReviewStage("effective")).toBe(false)
    })

    it("shows the unified approval responsibility without leaking role codes", () => {
        const order = {
            ownerName: "销售甲",
            primaryStatus: {
                code: "in_approval",
                label: "审批中",
                tone: "warning",
                ownerRole: "sales_order_approver",
                ownerUserName: "采购乙",
            },
        } as SalesOrderListItem

        expect(stageOwnerDisplay(order)).toBe("销售单审批 · 采购乙")
    })
})
