import { describe, expect, it } from "vitest"

import { mapCenter } from "./purchase-order-mapping"
import type { BackendCenter } from "./purchase-order-wire-types"

const center: BackendCenter = {
    id: "po-1", purchase_no: "PO-1", status: "EFFECTIVE", review_status: "APPROVED",
    version: 3, sales_order_id: "so-1", sales_order_no: "SO-1",
    supplier_id: "supplier-1", supplier_name: "供应商", purchase_type: "PHYSICAL",
    payment_term_code: "POSTPAY_NET30", fulfillment_responsibility: "WAREHOUSE",
    owner_user_id: "buyer-1", owner_name: "采购", payment_progress: "NONE",
    invoice_progress: "NONE", fulfillment_progress: "NONE", content_source: "REVISION",
    lines: [], totals: { gross: "100.00", net: "100.00", tax: "0.00" },
    allocations: [], changes: [], created_at: 1,
}

describe("采购变更入口", () => {
    it.each(["DRAFT", "IN_APPROVAL"])("已有 %s 变更时禁止重复发起", (status) => {
        const view = mapCenter({ ...center, changes: [{
            change_id: "change-1", status, base_revision_id: "revision-1",
            reason: "采购变更", created_at: 1,
        }] })
        expect(view.allowedActions).not.toContain("START_CHANGE")
        expect(view.actionBlockers).toContainEqual(expect.objectContaining({
            action: "START_CHANGE", code: "CHANGE_IN_PROGRESS",
        }))
    })

    it("变更生效后恢复入口", () => {
        const view = mapCenter({ ...center, changes: [{
            change_id: "change-1", status: "EFFECTIVE", base_revision_id: "revision-1",
            reason: "采购变更", created_at: 1,
        }] })
        expect(view.allowedActions).toContain("START_CHANGE")
        expect(view.actionBlockers).toEqual([])
    })
})
