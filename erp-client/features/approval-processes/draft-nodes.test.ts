import { describe, expect, it } from "vitest"

import {
    canMutateNodeStructure,
    orderNodesForSave,
    seedDraftNodes,
} from "./draft-nodes"
import { salesOrderEmptyDraft, salesOrderSavedDraft } from "./fixtures"
import { SALES_ORDER_PROCUREMENT_PURPOSE } from "./types"
import { buildReplaceNodesRequest } from "./write-payload"
import { createDraftSchema } from "./schema"

describe("draft nodes", () => {
    it("seeds an unsaved procurement slot for SalesOrder empty drafts", () => {
        const nodes = seedDraftNodes(
            "sales_order",
            salesOrderEmptyDraft().nodes,
        )
        expect(nodes).toHaveLength(1)
        expect(nodes[0]?.unsaved_purpose_slot).toBe(true)
        expect(nodes[0]?.node_purpose).toBe(SALES_ORDER_PROCUREMENT_PURPOSE)
        expect(nodes[0]?.node_id).toBeNull()
        expect(canMutateNodeStructure(nodes[0]!)).toBe(false)
    })

    it("does not seed procurement purpose for other document types", () => {
        const nodes = seedDraftNodes("stock_adjustment", [])
        expect(nodes).toEqual([])
        const voucher = seedDraftNodes("voucher_sales_order", [])
        expect(voucher).toEqual([])
    })

    it("locks the saved procurement purpose node from delete or copy", () => {
        const nodes = seedDraftNodes(
            "sales_order",
            salesOrderSavedDraft().nodes,
        )
        expect(canMutateNodeStructure(nodes[0]!)).toBe(false)
        expect(canMutateNodeStructure(nodes[1]!)).toBe(true)
    })

    it("forces the unsaved procurement slot to order 1 on first save", () => {
        const nodes = [
            {
                client_id: "a",
                node_id: null,
                node_name: "销售复核",
                assignee_user_id: "u2",
                assignee_name: "李四",
                node_purpose: null,
                unsaved_purpose_slot: false,
            },
            {
                client_id: "b",
                node_id: null,
                node_name: "采购确认",
                assignee_user_id: "u1",
                assignee_name: "张三",
                node_purpose: SALES_ORDER_PROCUREMENT_PURPOSE,
                unsaved_purpose_slot: true,
            },
        ]
        const ordered = orderNodesForSave("sales_order", nodes)
        const request = buildReplaceNodesRequest("1", ordered)
        expect(request.nodes[0]?.node_name).toBe("采购确认")
        expect(request.nodes[0]?.display_order).toBe(1)
        expect(JSON.stringify(request)).not.toContain("node_purpose")
        expect(request.nodes[0]).not.toHaveProperty("node_id")
    })

    it("requires an explicit draft source", () => {
        const missing = createDraftSchema.safeParse({
            name: "库存调整审批",
            draft_source: "",
        })
        expect(missing.success).toBe(false)
        const ok = createDraftSchema.safeParse({
            name: "库存调整审批",
            draft_source: "EMPTY",
        })
        expect(ok.success).toBe(true)
    })
})
