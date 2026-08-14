import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/features/work-items", () => ({
    listWorkItems: vi.fn(),
    mapWorkItemDto: vi.fn((dto: { id: string }) => ({
        workItemId: dto.id,
        status: "OPEN",
    })),
}))

vi.mock("./details", () => ({
    fetchConfirmationDetail: vi.fn(),
    fetchSalesOrderDetail: vi.fn(),
}))

import { listWorkItems } from "@/features/work-items"

import { fetchProcurementQueue } from "./queue"

const mockedListWorkItems = vi.mocked(listWorkItems)

const W02_QUEUE_CONTEXT_ID =
    "271b84b7bfdcdf958012858b2b57a09b1cbd9fd7e293122a59835b5d9e1d3560"
const W07_QUEUE_CONTEXT_ID =
    "89cbbe65b947589c15920e01adef5faf4cb6d33d951e120530b6e26e03311fdb"

beforeEach(() => {
    vi.clearAllMocks()
    mockedListWorkItems.mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 100,
        queue_context_id: W07_QUEUE_CONTEXT_ID,
    })
})

describe("fetchProcurementQueue", () => {
    it("does not send a W02 or URL queueContextId on the first list request", async () => {
        const view = await fetchProcurementQueue({
            scope: "mine",
            sort: "due_at",
            queueContextId: W02_QUEUE_CONTEXT_ID,
            currentWorkItemId: "52ab71bee03146d8b85d0ebf1ed12dcb",
        })

        expect(mockedListWorkItems).toHaveBeenCalledTimes(1)
        expect(mockedListWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({
                scope: "mine",
                workItemType: "PROCUREMENT_CONFIRMATION",
                sort: "due_asc",
                currentWorkItemId: "52ab71bee03146d8b85d0ebf1ed12dcb",
                queueContextId: undefined,
            }),
        )
        expect(view.context.queueContextId).toBe(W07_QUEUE_CONTEXT_ID)
    })

    it("does not send the client placeholder queue context", async () => {
        await fetchProcurementQueue({
            scope: "mine",
            queueContextId: "queue:procurement-confirmation:mine",
        })

        expect(mockedListWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({
                queueContextId: undefined,
            }),
        )
    })

    it("reuses the last server-issued context for the same W07 query", async () => {
        await fetchProcurementQueue(
            { scope: "mine", sort: "due_at" },
            W07_QUEUE_CONTEXT_ID,
        )

        expect(mockedListWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({
                scope: "mine",
                workItemType: "PROCUREMENT_CONFIRMATION",
                sort: "due_asc",
                queueContextId: W07_QUEUE_CONTEXT_ID,
            }),
        )
    })

    it("ignores a non-server listQueueContextId even if the caller passes one", async () => {
        await fetchProcurementQueue(
            { scope: "mine" },
            "queue:procurement-confirmation:mine",
        )

        expect(mockedListWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({
                queueContextId: undefined,
            }),
        )
    })
})
