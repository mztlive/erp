import { describe, expect, it } from "vitest"

import { mapRevisions } from "@/features/sales-orders/lib/sales-order-detail-mappers"

describe("mapRevisions", () => {
    it("maps header snapshots and line summaries from the revision payload", () => {
        const mapped = mapRevisions([
            {
                id: "rev-1",
                revision_no: 1,
                revision_source: "ERP_APPROVAL",
                content_hash: "abc",
                gross_amount: "60700.00",
                net_amount: "53716.81",
                tax_amount: "6983.19",
                effective_at: 1_787_654_280,
                created_at: 1_787_654_280,
                customer_name: "东方企业",
                contract_no: "123456",
                settlement_party_name: "集团结算中心",
                payment_term_name: "月结 30 天",
                invoice_type: "增值税专用发票",
                tax_point: "13",
                line_summary: "年货礼盒 共 1 项",
                lines: [
                    {
                        line_no: 1,
                        item_name: "年货礼盒",
                        spec: "10kg",
                        unit: "盒",
                        gross_amount: "60700.00",
                    },
                ],
            },
        ])

        expect(mapped).toHaveLength(1)
        expect(mapped[0]).toMatchObject({
            revisionNo: 1,
            customerSnapshot: "东方企业",
            contractRevisionLabel: "123456",
            settlementParty: "集团结算中心",
            paymentTerm: "月结 30 天",
            amountGross: "60700.00",
            amountNet: "53716.81",
            taxAmount: "6983.19",
            lineSummary: "年货礼盒 共 1 项",
            note: "审批生效",
        })
        expect(mapped[0]?.lines).toEqual([
            {
                lineNo: 1,
                name: "年货礼盒",
                spec: "10kg",
                unit: "盒",
                amountGross: "60700.00",
            },
        ])
    })

    it("hides identity-like spec snapshots so they do not render as 规格", () => {
        const mapped = mapRevisions([
            {
                id: "rev-1",
                revision_no: 1,
                revision_source: "ERP_APPROVAL",
                content_hash: "abc",
                gross_amount: "1000.00",
                net_amount: "870.00",
                tax_amount: "130.00",
                effective_at: 1_787_654_280,
                created_at: 1_787_654_280,
                lines: [
                    {
                        line_no: 1,
                        item_name: "奇乐融融A",
                        spec: "b8b18258a7444b7e82cb76faf16a3f68",
                        unit: "盒",
                        gross_amount: "1000.00",
                    },
                ],
            },
        ])

        expect(mapped[0]?.lines[0]).toMatchObject({
            name: "奇乐融融A",
            spec: undefined,
            unit: "盒",
        })
    })

    it("falls back to joining line names when line_summary is missing", () => {
        const mapped = mapRevisions([
            {
                id: "rev-2",
                revision_no: 2,
                revision_source: "SALES_CHANGE",
                content_hash: "def",
                gross_amount: "100.00",
                net_amount: "88.50",
                tax_amount: "11.50",
                effective_at: 1_787_654_280,
                created_at: 1_787_654_280,
                previous_revision_id: "rev-1",
                lines: [
                    {
                        line_no: 2,
                        item_name: "企业福利卡",
                        gross_amount: "40.00",
                    },
                    {
                        line_no: 1,
                        item_name: "年货礼盒",
                        gross_amount: "60.00",
                    },
                ],
            },
            {
                id: "rev-1",
                revision_no: 1,
                revision_source: "ERP_APPROVAL",
                content_hash: "abc",
                gross_amount: "60.00",
                net_amount: "53.10",
                tax_amount: "6.90",
                effective_at: 1_787_650_000,
                created_at: 1_787_650_000,
            },
        ])

        expect(mapped[0]?.lineSummary).toBe("年货礼盒、企业福利卡 共 2 项")
        expect(mapped[0]?.previousRevisionNo).toBe(1)
        expect(mapped[0]?.lines.map((line) => line.lineNo)).toEqual([1, 2])
    })
})
