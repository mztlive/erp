import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    getWorkItem,
    listWorkItems,
    mapWorkItemDto,
} from "@/features/work-items"
import { apiGet } from "@/lib/api"

import type { BackendReceivableAccount } from "./dto"
import { fetchFocusedCardFundsReviewTask } from "./queue"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
vi.mock("@/features/work-items", () => ({
    getWorkItem: vi.fn(),
    listWorkItems: vi.fn(),
    mapWorkItemDto: vi.fn(),
}))

const projection = {
    workItemId: "wi-13",
    workItemType: "CARD_FUNDS_REVIEW",
    businessObjectType: "receivable_account",
    businessObjectId: "ra-1",
    subjectVersion: "sor-1",
    taskVersion: "3",
    status: "OPEN",
    allowedActions: ["REASSIGN"],
    actionBlockers: [],
    priority: "HIGH",
    reasonLabel: "卡券期初待复核",
    impactSummary: "应收 100.00 元",
} as const

const account = {
    id: "ra-1",
    sales_order_id: "so-1",
    source_sales_order_revision_id: "sor-1",
    current_sales_order_revision_id: "sor-1",
    sales_order_no: "SO-1",
    sales_order_revision_no: 1,
    sales_order_snapshot_at: 1_700_000_000,
    account_seq: 1,
    customer_id: "customer-1",
    customer_name: "测试客户",
    counterparty_party_id: "party-1",
    counterparty_party_name: "测试结算主体",
    review_status: "opening_pending",
    gross_total: "100.00",
    settled_total: "0.00",
    open_total: "100.00",
    invoiceable_total: "100.00",
    invoiced_total: "0.00",
    open_invoiceable_total: "100.00",
    status: "open",
    version: 1,
    account_domain_version: "1",
    created_at: 1_700_000_000,
    entries: [],
    reviews: [],
    review_chain_version: "rcv:empty",
    next_review_no: 1,
    funds_fact_version: "ffv:empty",
    receipt_facts: [],
    invoice_facts: [],
    work_item: {} as never,
    active_review_type: "OPENING",
    allowed_actions: ["APPROVE", "REJECT"],
    action_blockers: [],
} satisfies BackendReceivableAccount

describe("fetchFocusedCardFundsReviewTask", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.mocked(getWorkItem).mockResolvedValue({} as never)
        vi.mocked(mapWorkItemDto).mockReturnValue(projection as never)
        vi.mocked(apiGet).mockResolvedValue(account as never)
    })

    it("按任务直读并且不复用父 W01 队列上下文", async () => {
        const view = await fetchFocusedCardFundsReviewTask("wi-13")

        expect(getWorkItem).toHaveBeenCalledWith("wi-13")
        expect(listWorkItems).not.toHaveBeenCalled()
        expect(apiGet).toHaveBeenCalledWith("/admin/receivable-accounts/ra-1", {
            work_item_id: "wi-13",
        })
        expect(view.context.queueContextId).toBe("focused-card-funds:wi-13")
        expect(view.current?.workItem.workItemId).toBe("wi-13")
    })
})
