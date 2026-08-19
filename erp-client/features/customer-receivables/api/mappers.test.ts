import { describe, expect, it } from "vitest"

import { projectReceipt } from "./mappers"
import type { BackendCustomerReceipt } from "./dto"

const seed = (): BackendCustomerReceipt => ({
    id: "cr-1",
    receipt_no: "SK-1",
    status: "draft",
    counterparty_party_id: "p1",
    customer_id: "c1",
    received_at: 1_700_000_000,
    amount: "100.00",
    bank_reference: "12345678",
    version: 1,
    created_at: 1_700_000_000,
    allocated_total: "0.00",
    unallocated_amount: "100.00",
    allocations: [],
})

describe("projectReceipt", () => {
    it("maps the created binding without turning it into a work item", () => {
        const row = projectReceipt({
            ...seed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                definition: {
                    id: "def-cr-1",
                    name: "客户回款审批",
                    version: 2,
                    nodes: [
                        { key: "n1", name: "回款复核", assignee_name: "张三" },
                    ],
                },
                instance: null,
                recent_history: [],
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(row.status).toBe("draft")
        expect(row.statusLabel).toBe("草稿")
        expect(row.allowedActions).toEqual(["VIEW_DETAIL", "CONTINUE_ALLOCATE"])
        expect(row.approval?.instance).toBeUndefined()
        expect(row.approval?.definition?.name).toBe("客户回款审批")
        expect(row.approval?.allowedActions).toEqual(["SUBMIT"])
    })

    it("converges pending review into in-approval and hides continue allocate", () => {
        const row = projectReceipt({
            ...seed(),
            status: "IN_APPROVAL",
            approval: {
                requirement: "PROCESS_REQUIRED",
                instance: {
                    id: "inst-cr-1",
                    status: "RUNNING",
                    current_round_no: 1,
                    current_node: "回款复核",
                    current_assignee: "张三",
                },
                recent_history: [],
                allowed_actions: ["CANCEL"],
            },
        })
        expect(row.status).toBe("in_approval")
        expect(row.statusLabel).toBe("审批中")
        expect(row.allowedActions).toEqual(["VIEW_DETAIL"])
        expect(row.approval?.instance?.currentAssigneeName).toBe("张三")
    })

    it("does not offer continue allocate on a posted receipt", () => {
        const row = projectReceipt({
            ...seed(),
            status: "posted",
        })
        expect(row.statusLabel).toBe("已过账")
        expect(row.allowedActions).toEqual([
            "VIEW_DETAIL",
            "REVERSE_RECEIPT",
            "REFUND",
        ])
        expect(row.approval).toBeUndefined()
    })
})
