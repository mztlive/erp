import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import {
    createAllocationSession,
    fetchAllocationSession,
    saveAllocationDraft,
    sessions,
} from "./session"

const mockedApiGet = vi.mocked(apiGet)

function account(id: string, salesOrderId: string, entryId: string) {
    return {
        id,
        sales_order_id: salesOrderId,
        sales_order_no: salesOrderId.toUpperCase(),
        account_seq: 1,
        customer_id: "customer-1",
        customer_name: "客户甲",
        counterparty_party_id: "party-1",
        counterparty_party_name: "结算主体甲",
        review_status: "not_applicable",
        gross_total: "100.00",
        settled_total: "0.00",
        open_total: "100.00",
        invoiceable_total: "100.00",
        invoiced_total: "0.00",
        open_invoiceable_total: "100.00",
        status: "open",
        version: 1,
        created_at: 1_700_000_000,
        entries: [
            {
                id: entryId,
                entry_type: "SALE",
                direction: "increase",
                amount: "100.00",
                due_date: "2026-09-30",
                source_document_id: salesOrderId,
                source_sequence: 1,
                posted_at: 1_700_000_000,
                offset_total: "0.00",
            },
        ],
        reviews: [],
    }
}

describe("allocation session sales-order scope", () => {
    beforeEach(() => {
        sessions.clear()
        mockedApiGet.mockReset()
        mockedApiGet.mockResolvedValue({
            items: [
                account("account-1", "so-1", "entry-1"),
                account("account-2", "so-2", "entry-2"),
            ],
            total: 2,
            page: 1,
            page_size: 100,
        })
    })

    it("keeps the pool fixed to the requested sales order when refreshed", async () => {
        const created = await createAllocationSession({
            mode: "receipt",
            counterpartyPartyId: "party-1",
            customerId: "customer-1",
            customerName: "客户甲",
            salesOrderId: "so-1",
        })

        expect(created.pool.map((target) => target.targetId)).toEqual([
            "entry-1",
        ])
        expect(created.pool[0]?.salesOrderId).toBe("so-1")

        const refreshed = await fetchAllocationSession(created.draftSessionId)
        expect(refreshed?.pool.map((target) => target.targetId)).toEqual([
            "entry-1",
        ])
        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/receivable-accounts",
            expect.objectContaining({ sales_order_id: "so-1" }),
        )
    })

    it("rejects a draft allocation target outside the fixed pool", async () => {
        const created = await createAllocationSession({
            mode: "receipt",
            counterpartyPartyId: "party-1",
            customerId: "customer-1",
            salesOrderId: "so-1",
        })

        await expect(
            saveAllocationDraft({
                draftSessionId: created.draftSessionId,
                editVersion: created.editVersion,
                fact: { receivedAt: "2026-08-24T10:00", amount: "100.00" },
                allocations: [
                    {
                        lineKey: "outside",
                        targetId: "entry-2",
                        targetKind: "receivable_entry",
                        label: "SO-2",
                        salesOrderNo: "SO-2",
                        openAmount: "100.00",
                        amount: "100.00",
                        baselineVersion: 1,
                    },
                ],
            }),
        ).rejects.toMatchObject({
            kind: "Validation",
            message: "分配目标不属于本次核销范围，请刷新后重新选择。",
        })
    })
})
