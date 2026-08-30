import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
} from "@/features/customer-quality/api"
import { apiGet } from "@/lib/api"

import { fetchCustomerCenter } from "./center"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

vi.mock("@/features/customer-quality/api", () => ({
    fetchCustomerQuality: vi.fn(),
    fetchCustomerQualityPeriodPolicy: vi.fn(),
}))

const apiGetMock = vi.mocked(apiGet)
const qualityMock = vi.mocked(fetchCustomerQuality)
const qualityPolicyMock = vi.mocked(fetchCustomerQualityPeriodPolicy)

const profile = {
    id: "customer-1",
    party_id: "party-1",
    customer_no: "KH-001",
    status: "active",
    version: 3,
    party_version: 4,
    current_revision: {
        id: "revision-2",
        revision_no: 2,
        legal_name: "甲客户",
        short_name: "甲",
        change_reason: "资料修订",
        version: 2,
        created_at: 1_787_910_000,
    },
    revisions: [
        {
            id: "revision-2",
            revision_no: 2,
            legal_name: "甲客户",
            short_name: "甲",
            change_reason: "资料修订",
            version: 2,
            created_at: 1_787_910_000,
        },
    ],
    assignments: [],
    contacts: [],
    addresses: [],
    bank_accounts: [],
    sensitive_fields: [],
    allowed_actions: ["edit"],
    action_blockers: [],
    updated_at: 1_787_910_000,
}

const receivableSummary = {
    receivable_balance: "1200.50",
    overdue_amount: "100.00",
    open_invoiceable_total: "1100.00",
    earliest_overdue_date: "2026-08-20",
    projected_at: 1_787_910_500,
}

describe("fetchCustomerCenter", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
        qualityMock.mockReset()
        qualityPolicyMock.mockReset()
        qualityPolicyMock.mockResolvedValue({
            hasDefault: false,
            timezone: "Asia/Shanghai",
        })
    })

    it("loads bounded server projections without scanning domain lists", async () => {
        apiGetMock.mockImplementation(async (path) => {
            if (path === "/admin/customer-profiles/customer-1") return profile
            if (
                path === "/admin/customer-profiles/customer-1/related-summary"
            ) {
                return {
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
                }
            }
            if (
                path ===
                "/admin/customer-profiles/customer-1/receivable-summary"
            ) {
                return receivableSummary
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const view = await fetchCustomerCenter("customer-1")

        expect(view).toMatchObject({
            customerId: "customer-1",
            metrics: {
                activeContractCount: 2,
                inProgressSalesOrderCount: 3,
                receivableBalance: "1200.5",
                overdueAmount: "100",
            },
            partitions: { related: "ok", settlement: "ok" },
        })
        expect(view?.contracts).toHaveLength(1)
        expect(view?.salesOrders).toHaveLength(1)
        expect(
            apiGetMock.mock.calls.filter(([path]) =>
                [
                    "/admin/contracts",
                    "/admin/sales-orders",
                    "/admin/receivable-accounts",
                ].includes(path),
            ),
        ).toHaveLength(0)
        expect(qualityMock).not.toHaveBeenCalled()
    })

    it("keeps identity and receivables when the related projection fails", async () => {
        apiGetMock.mockImplementation(async (path) => {
            if (path === "/admin/customer-profiles/customer-1") return profile
            if (path.endsWith("/related-summary")) {
                throw new Error("关联投影不可用")
            }
            if (path.endsWith("/receivable-summary")) {
                return receivableSummary
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const view = await fetchCustomerCenter("customer-1")

        expect(view).toMatchObject({
            customerId: "customer-1",
            metrics: {
                activeContractCount: null,
                inProgressSalesOrderCount: null,
                receivableBalance: "1200.5",
                overdueAmount: "100",
            },
            partitions: { identity: "ok", related: "error", settlement: "ok" },
        })
        expect(view?.contracts).toEqual([])
        expect(view?.salesOrders).toEqual([])
    })
})
