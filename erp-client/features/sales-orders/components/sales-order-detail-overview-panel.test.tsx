import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import { mapListItemFromBackend } from "@/features/sales-orders/lib/sales-order-detail-mappers"
import { OverviewPanel } from "./sales-order-detail-overview-panel"

afterEach(() => {
    cleanup()
})

const order = (): SalesOrderDetailView => ({
    ...mapListItemFromBackend(
        {
            id: "so-1",
            order_no: "XS202608230001",
            business_type: "VOUCHER",
            origin_system: "ERP",
            customer_id: "customer-1",
            contract_id: "contract-1",
            commercial_status: "PENDING_REVIEW",
            review_status: "IN_APPROVAL",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_COLLECTED",
            invoice_progress: "NOT_INVOICED",
            close_status: "OPEN",
            version: 2,
            created_at: 1,
            updated_at: 1,
            stage: {
                code: "in_approval",
                label: "审批中",
                tone: "warning",
            },
        },
        {
            customerName: "客户甲",
            contractNumber: "HT-2026-001",
            contractRevisionLabel: "HT-2026-001@v2",
            settlementEntity: "结算主体甲",
            welfareScene: "ANNUAL_GIFT_BAG",
            paymentTerms: "POSTPAY_NET30",
            taxRatePercent: "6.00",
            fulfillmentDeadline: "2027-12-31",
            targetMallName: "员工福利商城",
            receivableDueDate: "2026-09-30",
            customerContact: "联系人甲",
            remark: "客户要求分批发放",
            lineItems: [
                {
                    id: "line-1",
                    name: "节日卡券",
                    quantity: "2",
                    unit: "张",
                    unitPriceGross: "90.00",
                    amountGross: "180.00",
                    faceValue: "100.00",
                    giftRate: "11.11",
                    cardForm: "电子卡",
                },
            ],
        },
    ),
    acceptance: null,
    permissionVersion: "pv-test",
    sourceAsOf: "2026-08-23T00:00:00Z",
    queriedAt: "2026-08-23T00:00:00Z",
})

describe("OverviewPanel", () => {
    it("renders the creation fields and line inputs on the detail page", () => {
        render(<OverviewPanel order={order()} />)

        expect(screen.getByText("HT-2026-001@v2")).toBeTruthy()
        expect(screen.getByText("结算主体甲")).toBeTruthy()
        expect(screen.getByText("年节礼包")).toBeTruthy()
        expect(screen.getByText("货到 30 天")).toBeTruthy()
        expect(screen.getByText("员工福利商城")).toBeTruthy()
        expect(screen.getByText("2026-09-30")).toBeTruthy()
        expect(screen.getByText("6.00%")).toBeTruthy()
        expect(screen.getByText("客户要求分批发放")).toBeTruthy()
        expect(screen.getByText("尚未生效")).toBeTruthy()
        expect(screen.queryByText("v2")).toBeNull()
        expect(
            screen.getByRole("columnheader", { name: "含税单价" }),
        ).toBeTruthy()
        expect(screen.getByRole("columnheader", { name: "面值" })).toBeTruthy()
        expect(screen.getByRole("columnheader", { name: "配赠" })).toBeTruthy()
        expect(
            screen.getByRole("columnheader", { name: "卡形态" }),
        ).toBeTruthy()
        expect(screen.getByText("11.11%")).toBeTruthy()
        expect(screen.getByText("电子卡")).toBeTruthy()
    })
})
