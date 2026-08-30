import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet } from "@/lib/api"
import {
    fetchCostEntriesForRow,
    fetchProfitLossView,
} from "@/features/actual-profit-loss/api"
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
