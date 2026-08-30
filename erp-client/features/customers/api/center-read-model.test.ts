import { describe, expect, it } from "vitest"

import {
    decodeCustomerCenterReceivable,
    decodeCustomerCenterRelated,
} from "./center-read-model"

describe("customer center read-model contracts", () => {
    it("decodes the bounded related summary", () => {
        expect(
            decodeCustomerCenterRelated({
                active_contract_count: 2,
                in_progress_sales_order_count: 3,
                contracts: [
                    {
                        id: "contract-1",
                        contract_no: "HT-001",
                        status: "EFFECTIVE",
                    },
                ],
                sales_orders: [
                    {
                        id: "sales-1",
                        order_no: "SO-001",
                        commercial_status: "EFFECTIVE",
                        close_status: "NOT_SATISFIED",
                        created_at: 1_787_910_400,
                    },
                ],
                projected_at: 1_787_910_500,
            }),
        ).toMatchObject({
            active_contract_count: 2,
            in_progress_sales_order_count: 3,
        })
    })

    it("keeps amounts as normalized decimal strings", () => {
        expect(
            decodeCustomerCenterReceivable({
                receivable_balance: "1200.50",
                overdue_amount: "100.00",
                open_invoiceable_total: "1100.00",
                earliest_overdue_date: "2026-08-20",
                projected_at: 1_787_910_500,
            }),
        ).toMatchObject({
            receivable_balance: "1200.5",
            overdue_amount: "100",
            open_invoiceable_total: "1100",
        })
    })

    it.each([
        ["number amount", 1200],
        ["invalid decimal", "12.345"],
        ["negative balance", "-1"],
    ])("rejects %s", (_label, receivableBalance) => {
        expect(() =>
            decodeCustomerCenterReceivable({
                receivable_balance: receivableBalance,
                overdue_amount: "0",
                open_invoiceable_total: "0",
                earliest_overdue_date: null,
                projected_at: 1_787_910_500,
            }),
        ).toThrow("客户应收摘要响应契约不匹配")
    })

    it("rejects unknown fields instead of hiding contract drift", () => {
        expect(() =>
            decodeCustomerCenterRelated({
                active_contract_count: 0,
                in_progress_sales_order_count: 0,
                contracts: [],
                sales_orders: [],
                projected_at: 1_787_910_500,
                next_page: 2,
            }),
        ).toThrow("客户关联摘要响应契约不匹配")
    })
})
