import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import { loadReceipts } from "./loaders"

const mockedApiGet = vi.mocked(apiGet)

describe("customer receivables pagination", () => {
    beforeEach(() => mockedApiGet.mockReset())

    it("loads every receipt page before sales-order allocation filtering", async () => {
        mockedApiGet.mockImplementation(async (_path, query) => {
            const page = Number(query?.page ?? 1)
            return {
                items: [{ id: `receipt-${page}` }],
                total: 2,
                page,
                page_size: 100,
            }
        })

        const result = await loadReceipts({ view: "receipt" })

        expect(result.items.map((item) => item.id)).toEqual([
            "receipt-1",
            "receipt-2",
        ])
        expect(mockedApiGet).toHaveBeenCalledTimes(2)
    })
})
