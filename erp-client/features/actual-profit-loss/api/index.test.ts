import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet, apiPost } from "@/lib/api"
import {
    fetchCostEntriesForRow,
    fetchProfitLossView,
    startProfitLossExport,
} from "@/features/actual-profit-loss/api"
import { makeQuery } from "@/features/actual-profit-loss/hooks/test-fixtures"
import type { ProfitLossQuery } from "@/features/actual-profit-loss/types"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

const mockedApiGet = vi.mocked(apiGet)

describe("actual profit-loss server pagination", () => {
    beforeEach(() => {
        mockedApiGet.mockReset()
        mockedApiGet.mockResolvedValue({})
    })

    it("sends both page and page_size to the backend", async () => {
        const query: ProfitLossQuery = {
            from: "2026-08-01",
            to: "2026-08-31",
            periodBasis: "sales_revenue_recognition_date",
            scopeId: "all",
            coverage: "covered",
            dimension: "sales_order",
            sort: "profit_desc",
            page: 2,
            pageSize: 50,
        }

        await fetchProfitLossView(query)

        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/actual-profit-loss",
            expect.objectContaining({ page: 2, page_size: 50 }),
        )
    })

    it("fails the drilldown when a formal cost fact cannot be loaded", async () => {
        mockedApiGet.mockRejectedValueOnce(new Error("成本事实不存在"))

        await expect(fetchCostEntriesForRow(["cost-1"])).rejects.toThrow(
            "成本事实不存在",
        )
    })
})

describe("profit-loss full export and cost enum contract", () => {
    it("exports all applied filters without trusting page amounts or scope permissions", async () => {
        const query = makeQuery({
            customerId: "c-1",
            salesOrderId: "so-1",
            q: "[客户]",
            benefitScenario: "常规福利",
            costTypes: ["delivery"],
            dimension: "customer",
            page: 2,
        })
        vi.mocked(apiPost).mockResolvedValue({
            csvContent: "全量结果",
            fileName: "盈亏.csv",
            rowCount: 25,
            generatedAt: "2026-09-11T01:00:00Z",
        })
        const result = await startProfitLossExport({ query })
        expect(apiPost).toHaveBeenCalledWith(
            "/admin/actual-profit-loss/exports",
            expect.objectContaining({
                customer_id: "c-1",
                sales_order_id: "so-1",
                benefit_scenario: "常规福利",
                cost_types: "delivery",
                dimension: "customer",
                q: "[客户]",
                page: 2,
            }),
        )
        expect(result.rowCount).toBe(25)
    })
    it("renders persisted snake-case cost enums as Chinese labels", async () => {
        vi.mocked(apiGet).mockResolvedValue({
            id: "ce-1",
            cost_type: "product",
            cost_stage: "actual",
            cost_scope: "non_voucher_fulfillment",
            gross_amount: "113",
            net_amount: "100",
            tax_amount: "13",
            input_tax_rate: "0.13",
            occurred_at: 1,
            allocations: [],
        })
        const [entry] = await fetchCostEntriesForRow(["ce-1"])
        expect(entry).toMatchObject({
            stage: "ACTUAL",
            stageLabel: "实际",
            costScopeLabel: "非卡券履约",
            costTypeLabel: "商品",
        })
    })
})
