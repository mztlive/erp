import { describe, expect, it } from "vitest"

import type { BackendSalesOrderDetail } from "@/features/sales-orders/api/contracts"

import {
    normalizeObjectType,
    salesOrderDocumentFacts,
    shouldLoadDocumentFacts,
} from "./document-facts"

describe("normalizeObjectType", () => {
    it("accepts snake_case and PascalCase object types", () => {
        expect(normalizeObjectType("sales_order")).toBe("sales_order")
        expect(normalizeObjectType("SalesOrder")).toBe("sales_order")
        expect(normalizeObjectType("customer_receipt")).toBe("customer_receipt")
    })
})

describe("shouldLoadDocumentFacts", () => {
    it("skips fetch when the work item already has a brief", () => {
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "sales_order",
                hasSummary: true,
            }),
        ).toBe(false)
    })

    it("loads sales orders and receipts when the brief is empty", () => {
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "sales_order",
                hasSummary: false,
            }),
        ).toBe(true)
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "SalesOrder",
                hasSummary: false,
            }),
        ).toBe(true)
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "customer_receipt",
                hasSummary: false,
            }),
        ).toBe(true)
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "stock_adjustment",
                hasSummary: false,
            }),
        ).toBe(false)
    })
})

describe("salesOrderDocumentFacts", () => {
    it("maps the in-review submission header and first lines", () => {
        const facts = salesOrderDocumentFacts({
            id: "so-1",
            order_no: "XS-1",
            business_type: "GOODS_SERVICE",
            origin_system: "ERP",
            customer_id: "c1",
            settlement_party_id: "p1",
            commercial_status: "IN_REVIEW",
            review_status: "IN_APPROVAL",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_STARTED",
            invoice_progress: "NOT_STARTED",
            close_status: "OPEN",
            version: 1,
            created_at: 1,
            owner_user_id: "u1",
            purchase_order_count: 0,
            settled_total: "0",
            invoiced_total: "0",
            lines: [],
            submissions: [
                {
                    id: "sub-1",
                    submission_no: 1,
                    status: "IN_REVIEW",
                    business_type: "GOODS_SERVICE",
                    customer_name: "华东纸业",
                    contract_no: "HT-1",
                    settlement_party_name: "华东结算",
                    payment_term_code: "PREPAY_30",
                    payment_term_name: "先款 30%",
                    invoice_type: "VAT",
                    tax_point: "13",
                    gross_amount: "12800",
                    net_amount: "11327.43",
                    tax_amount: "1472.57",
                    submitted_by: "u1",
                    submitted_at: 1,
                    created_at: 1,
                    lines: [
                        {
                            id: "l1",
                            sales_order_line_id: "sl1",
                            line_no: 1,
                            line_type: "GOODS_SERVICE",
                            gross_amount: "10000",
                            net_amount: "8850",
                            tax_amount: "1150",
                            sales_tax_rate: "0.13",
                            item_name_snapshot: "办公椅",
                            quantity: "20",
                            unit_snapshot: "件",
                        },
                    ],
                },
            ],
            revisions: [],
            stage: { code: "in_approval", label: "审批中", tone: "info" },
            close_eligibility: {
                fulfillment_complete: false,
                receivable_settled: false,
                invoice_complete: false,
                eligible_to_close: false,
                blockers: [],
                note: "",
            },
            can_start_sales_change_order: false,
        } as BackendSalesOrderDetail)

        expect(facts?.counterparty).toBe("华东纸业")
        expect(facts?.impact).toBe("不审批则销售单不能生效、不能履约")
        expect(facts?.sections.some((row) => row.label === "含税金额")).toBe(
            true,
        )
        expect(facts?.lines[0]?.title).toBe("办公椅")
        expect(facts?.listSummary).toContain("华东纸业")
    })
})
