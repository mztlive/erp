import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, waitFor, cleanup } from "@testing-library/react"

vi.mock("next/navigation", () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => new URLSearchParams() as unknown as ReadonlyURLSearchParams),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

vi.mock("@/features/fulfillment-operations/api", () => ({
    fetchFulfillmentQueue: vi.fn(),
    saveFulfillmentOperation: vi.fn(),
    postFulfillmentOperation: vi.fn(),
    resolveUnknownFulfillmentResult: vi.fn(),
}))

import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"
import {
    fetchFulfillmentQueue,
    postFulfillmentOperation,
    saveFulfillmentOperation,
} from "@/features/fulfillment-operations/api"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useFulfillmentOperationsController } from "./use-fulfillment-operations-controller"
import { makeOperation, makePostedOutcome, makeQueueView } from "./test-data"

const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)
const mockedRouter = vi.mocked(useRouter)
const mockedFetchQueue = vi.mocked(fetchFulfillmentQueue)
const mockedSave = vi.mocked(saveFulfillmentOperation)
const mockedPost = vi.mocked(postFulfillmentOperation)

const WAREHOUSE_FILTERS = {
    role: "warehouse",
} as import("@/features/fulfillment-operations/api").FulfillmentQueueFilters

function setupRouter() {
    const router = { push: vi.fn(), replace: vi.fn(), back: vi.fn() }
    mockedRouter.mockReturnValue(
        router as unknown as ReturnType<typeof useRouter>,
    )
    return router
}

function renderController({
    url = "lane=warehouse&currentOperationId=op_1",
    lane = "warehouse",
    autoNextExplicit = undefined,
}: {
    url?: string
    lane?: "warehouse" | "procurement" | null
    autoNextExplicit?: string | null
} = {}) {
    mockedSearchParams.mockReturnValue(new URLSearchParams(url) as unknown as ReadonlyURLSearchParams)
    mockedPathname.mockReturnValue("/fulfillment")
    const router = setupRouter()
    const client = createFreshQueryClient()
    const utils = renderHookWithProviders(
        () =>
            useFulfillmentOperationsController({
                roleValue: "warehouse",
                filters: WAREHOUSE_FILTERS,
                lane,
                autoNextExplicit,
            }),
        { queryClient: client },
    )
    return { ...utils, router, client }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useFulfillmentOperationsController", () => {
    it("loads the queue with the filters and hydrates the draft of the current operation", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result } = renderController()
        expect(result.current.queueQuery.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )
        expect(mockedFetchQueue).toHaveBeenCalledWith(WAREHOUSE_FILTERS)
        expect(result.current.operation?.operationId).toBe("op_1")
        expect(result.current.draft).toEqual(view.operations[0].draft)
        expect(result.current.dirty).toBe(false)
        expect(result.current.canExecute).toBe(true)
        expect(result.current.completed).toBe(false)
    })

    it("marks the queue completed when there are no operations", async () => {
        mockedFetchQueue.mockResolvedValue(makeQueueView([]))

        const { result } = renderController({ url: "lane=warehouse" })
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )
        expect(result.current.completed).toBe(true)
        expect(result.current.operation).toBeUndefined()
        expect(result.current.draft).toBeNull()
    })

    it("saves the draft, clears dirty and reports the save message", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)
        mockedSave.mockResolvedValue({ editVersion: 4 })

        const { result } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        act(() => {
            result.current.updateDraft(result.current.draft!)
        })
        expect(result.current.dirty).toBe(true)

        let saved: boolean | undefined
        await act(async () => {
            saved = await result.current.handleSave()
        })
        expect(saved).toBe(true)
        expect(mockedSave).toHaveBeenCalledTimes(1)
        expect(mockedSave.mock.calls[0][0]).toMatchObject({
            operationId: "op_1",
            expectedDocumentVersion: 3,
            expectedSourceVersion: "sv_1",
            idempotencyKey: expect.stringContaining(
                "w09:op_1:3:save:",
            ),
        })
        expect(result.current.dirty).toBe(false)
        expect(result.current.saveMessage).toBe("草稿已保存")
    })

    it("reports the error message when saving fails", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)
        mockedSave.mockRejectedValue(new Error("数据已更新，请刷新后重试"))

        const { result } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        let saved: boolean | undefined
        await act(async () => {
            saved = await result.current.handleSave()
        })
        expect(saved).toBe(false)
        expect(result.current.actionError).toBe(
            "数据已更新，请刷新后重试",
        )
    })

    it("posts the current draft and shows the succeeded result", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)
        mockedPost.mockResolvedValue({
            status: "succeeded",
            outcome: makePostedOutcome(),
        })

        const { result } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )
        expect(result.current.canPost).toBe(true)

        await act(async () => {
            await result.current.handlePost()
        })
        expect(mockedPost.mock.calls[0][0]).toMatchObject({
            operationId: "op_1",
            idempotencyKey: expect.stringContaining(
                "w09:op_1:3:post:",
            ),
        })
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.lastResult).toMatchObject({
            status: "succeeded",
            title: "已入库",
            reference: "RK-2026-001",
        })
    })

    it("keeps an unknown post result on the item for later resolution", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)
        mockedPost.mockResolvedValue({
            status: "unknown",
            message: "处理结果待确认，请勿重复提交",
            idempotencyKey: "w09:op_1:3:post:key",
        })

        const { result } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        await act(async () => {
            await result.current.handlePost()
        })
        expect(result.current.lastResult).toMatchObject({
            status: "unknown",
            stayOnItem: true,
            pendingIdempotencyKey: "w09:op_1:3:post:key",
        })
        // 结果待确认时草稿表单锁定，避免重复提交
        expect(result.current.lastResult?.status).toBe("unknown")
    })

    it("blocks navigation while the draft is dirty", async () => {
        const view = makeQueueView(
            [
                makeOperation({ operationId: "op_1" }),
                makeOperation({ operationId: "op_2" }),
            ],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result, router } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        act(() => {
            result.current.updateDraft(result.current.draft!)
        })
        act(() => {
            result.current.handleNavigate(1)
        })
        expect(router.replace).not.toHaveBeenCalled()
        expect(result.current.actionError).toBe(
            "有未保存修改，请先保存或放弃后再切换",
        )
    })

    it("navigates to the next operation when the draft is clean", async () => {
        const view = makeQueueView(
            [
                makeOperation({ operationId: "op_1" }),
                makeOperation({ operationId: "op_2" }),
            ],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result, router } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        act(() => {
            result.current.handleNavigate(1)
        })
        expect(router.replace).toHaveBeenCalledWith(
            "/fulfillment?lane=warehouse&currentOperationId=op_2",
            { scroll: false },
        )
    })

    it("refuses filter changes while the draft is dirty", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result, router } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )

        act(() => {
            result.current.updateDraft(result.current.draft!)
        })
        act(() => {
            result.current.setTypeFilter("RECEIPT")
        })
        expect(router.replace).not.toHaveBeenCalled()
        expect(result.current.actionError).toBe(
            "有没保存的修改，先保存或放弃再切换类型",
        )
    })

    it("syncs the autoNext session preference into the URL", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result, router } = renderController()
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )
        expect(result.current.autoNext).toBe(true)

        act(() => {
            result.current.setAutoNext(false)
        })
        expect(result.current.autoNext).toBe(false)
        expect(router.replace).toHaveBeenCalledWith(
            "/fulfillment?lane=warehouse&currentOperationId=op_1&autoNext=0",
            { scroll: false },
        )
    })

    it("respects the explicit autoNext=0 URL param over the session default", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { result } = renderController({
            url: "lane=warehouse&currentOperationId=op_1&autoNext=0",
            autoNextExplicit: "0",
        })
        await waitFor(() =>
            expect(result.current.queueQuery.isPending).toBe(false),
        )
        expect(result.current.autoNext).toBe(false)
    })

    it("settles lane and currentOperationId into the URL after the queue loads", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { router } = renderController({
            url: "",
            lane: "warehouse",
        })
        await waitFor(() =>
            expect(router.replace).toHaveBeenCalled(),
        )
        expect(router.replace).toHaveBeenCalledWith(
            "/fulfillment?lane=warehouse&currentOperationId=op_1",
            { scroll: false },
        )
    })

    it("does not write a lane into the URL for neutral deep links", async () => {
        const view = makeQueueView(
            [makeOperation({ operationId: "op_1" })],
            { currentOperationId: "op_1" },
        )
        mockedFetchQueue.mockResolvedValue(view)

        const { router } = renderController({
            url: "currentOperationId=op_1",
            lane: null,
        })
        await waitFor(() => {
            expect(mockedFetchQueue).toHaveBeenCalled()
        })
        expect(router.replace).not.toHaveBeenCalled()
    })
})
