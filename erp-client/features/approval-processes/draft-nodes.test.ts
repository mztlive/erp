import { describe, expect, it } from "vitest"

import {
    canMutateNodeStructure,
    orderNodesForSave,
    seedDraftNodes,
} from "./draft-nodes"
import { salesOrderEmptyDraft, salesOrderSavedDraft } from "./fixtures"
import { createDraftSchema } from "./schema"
import { buildReplaceNodesRequest } from "./write-payload"

describe("draft nodes", () => {
    it("seeds a deletable default node for SalesOrder empty drafts", () => {
        const nodes = seedDraftNodes(
            "sales_order",
            salesOrderEmptyDraft().nodes,
        )
        expect(nodes).toHaveLength(1)
        expect(nodes[0]?.node_name).toBe("采购确认")
        expect(nodes[0]?.unsaved_purpose_slot).toBe(false)
        expect(nodes[0]?.node_purpose).toBeNull()
        expect(nodes[0]?.node_id).toBeNull()
        expect(canMutateNodeStructure(nodes[0]!)).toBe(true)
    })

    it("does not seed a default node for other document types", () => {
        const nodes = seedDraftNodes("stock_adjustment", [])
        expect(nodes).toEqual([])
        const voucher = seedDraftNodes("voucher_sales_order", [])
        expect(voucher).toEqual([])
    })

    it("allows deleting a previously locked sales order node", () => {
        const nodes = seedDraftNodes(
            "sales_order",
            salesOrderSavedDraft().nodes,
        )
        expect(canMutateNodeStructure(nodes[0]!)).toBe(true)
        expect(canMutateNodeStructure(nodes[1]!)).toBe(true)
    })

    it("keeps the editor order when saving sales order nodes", () => {
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
                node_purpose: null,
                unsaved_purpose_slot: false,
            },
        ]
        const ordered = orderNodesForSave("sales_order", nodes)
        const request = buildReplaceNodesRequest("1", ordered)
        expect(request.nodes[0]?.node_name).toBe("销售复核")
        expect(request.nodes[0]?.display_order).toBe(1)
        expect(request.nodes[1]?.node_name).toBe("采购确认")
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
