import { expect, it } from "vitest"
import type { BackendSalesOrderDetail } from "../api/contracts"
import { mapDetailToListItem } from "./sales-order-detail-projection-mappers"
import { buildSalesOrdersListCsv } from "./sales-orders-list-csv"

it("详情和打印使用的同一投影忽略编辑人与提交人，导出保留显式销售负责人", () => {
    const detail = {
        id: "order-1",
        order_no: "XS-001",
        customer_id: "customer-1",
        business_type: "GOODS_SERVICE",
        origin_system: "ERP",
        commercial_status: "DRAFT",
        review_status: "NOT_SUBMITTED",
        fulfillment_progress: "NOT_STARTED",
        collection_progress: "NOT_COLLECTED",
        invoice_progress: "NOT_INVOICED",
        close_status: "NOT_SATISFIED",
        created_at: 1800000000,
        version: 1,
        owner_user_id: "sales-1",
        owner_user_name: "负责销售甲",
        stage: { code: "draft", label: "待提交", tone: "neutral" },
        working_copy: { editor_user_id: "editor-2", lines: [] },
        submissions: [
            { submission_no: 1, submitted_by: "submitter-3", lines: [] },
        ],
        revisions: [],
        purchase_creation_access: { allowed: false, task_count: 0 },
    } as unknown as BackendSalesOrderDetail
    const row = mapDetailToListItem(detail, {
        ownerUserId: "stale-owner",
        ownerName: "旧姓名",
    })
    expect(row.ownerUserId).toBe("sales-1")
    expect(row.ownerName).toBe("负责销售甲")
    const csv = buildSalesOrdersListCsv(
        [row],
        new Date("2026-09-13T00:00:00Z"),
    ).content
    expect(csv).toContain("负责销售甲")
    expect(csv).not.toContain("旧姓名")
    expect(csv).not.toContain("editor-2")
})
