import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import {
    completeImportConfirmation,
    executeImportCommand,
    fetchImportBatchDetail,
    fetchImportBatchList,
    fetchImportIssues,
} from "@/features/import-opening/api/legacy-import"
import {
    importOpeningKeys,
    useImportBatchDetailQuery,
    useImportBatchListQuery,
    useImportConfirmationOperations,
    useImportExecutionOperations,
    useImportIssuesQuery,
} from "@/features/import-opening/hooks/queries"
import type {
    ImportBatchListQuery,
    ImportBatchListView,
    ImportBatchView,
    ImportExecutionResult,
    ImportIssuePage,
} from "@/features/import-opening/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

vi.mock("@/features/import-opening/api/legacy-import", () => ({
    fetchImportBatchList: vi.fn(),
    fetchImportBatchDetail: vi.fn(),
    fetchImportIssues: vi.fn(),
    completeImportConfirmation: vi.fn(),
    executeImportCommand: vi.fn(),
}))

const listQuery: ImportBatchListQuery = {
    environment: "VALIDATION",
    objectType: "all",
    page: 1,
    pageSize: 20,
}

const listView: ImportBatchListView = {
    metrics: {
        pendingValidate: 1,
        pendingConfirm: 2,
        applying: 3,
        failedOrPartial: 4,
    },
    rows: [],
    totalCount: 0,
    queriedAt: "2026-08-14T00:00:00.000Z",
}

const detailView = { batchId: "b1", batchNo: "B-001" } as ImportBatchView

const issuePage: ImportIssuePage = {
    rows: [],
    totalCount: 0,
    issueVersion: "issv-b1-1",
    queriedAt: "2026-08-14T00:00:00.000Z",
}

const execResult: ImportExecutionResult = {
    action: "START_APPLY",
    resultStatus: "STARTED",
    batchId: "b1",
    batchStatus: "APPLYING",
    batchVersion: "1",
    backgroundJobId: "job1",
    backgroundJobStatus: "running",
    backgroundJobVersion: "1",
    affectedItems: 5,
    nextStep: "MONITOR_PROGRESS",
    auditReceipt: "r1",
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useImportBatchListQuery", () => {
    it("calls the api with the query and exposes the data", async () => {
        vi.mocked(fetchImportBatchList).mockResolvedValue(listView)
        const { result } = renderHookWithProviders(() =>
            useImportBatchListQuery(listQuery),
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(listView)
        expect(fetchImportBatchList).toHaveBeenCalledWith(listQuery)
    })

    it("keeps a stable cache entry for structurally equal params", async () => {
        vi.mocked(fetchImportBatchList).mockResolvedValue(listView)
        const client = createFreshQueryClient()
        let params: ImportBatchListQuery = listQuery
        const { rerender } = renderHookWithProviders(
            () => useImportBatchListQuery(params),
            { queryClient: client },
        )
        await waitFor(() =>
            expect(fetchImportBatchList).toHaveBeenCalledTimes(1),
        )

        params = { ...listQuery, page: 1 }
        rerender()
        await waitFor(() =>
            expect(client.getQueryCache().getAll()).toHaveLength(1),
        )
        expect(fetchImportBatchList).toHaveBeenCalledTimes(1)
        expect(
            client
                .getQueryCache()
                .find({ queryKey: importOpeningKeys.list(listQuery) }),
        ).toBeDefined()

        params = { ...listQuery, page: 2 }
        rerender()
        await waitFor(() =>
            expect(fetchImportBatchList).toHaveBeenCalledTimes(2),
        )
        expect(fetchImportBatchList).toHaveBeenLastCalledWith({
            ...listQuery,
            page: 2,
        })
    })

    it("surfaces the error state on failure", async () => {
        vi.mocked(fetchImportBatchList).mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useImportBatchListQuery(listQuery),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual(new Error("boom"))
        expect(result.current.data).toBeUndefined()
    })
})

describe("useImportBatchDetailQuery", () => {
    it("stays disabled without a batchId and does not call the api", async () => {
        const { result } = renderHookWithProviders(() =>
            useImportBatchDetailQuery(undefined),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(fetchImportBatchDetail).not.toHaveBeenCalled()
    })

    it("fetches the detail for a given context", async () => {
        vi.mocked(fetchImportBatchDetail).mockResolvedValue(detailView)
        const context = { batchId: "b1" }
        const { result } = renderHookWithProviders(() =>
            useImportBatchDetailQuery(context),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(detailView)
        expect(fetchImportBatchDetail).toHaveBeenCalledWith(context)
    })
})

describe("useImportIssuesQuery", () => {
    const query = {
        batchId: "b1",
        issueCode: "all" as const,
        objectType: "all" as const,
        rowStatus: "all" as const,
        page: 1,
        pageSize: 20,
    }

    it("is disabled when batchId is empty", () => {
        vi.mocked(fetchImportIssues).mockResolvedValue(issuePage)
        renderHookWithProviders(() =>
            useImportIssuesQuery({ ...query, batchId: "" }),
        )
        expect(fetchImportIssues).not.toHaveBeenCalled()
    })

    it("fetches issues for a batch and exposes the page", async () => {
        vi.mocked(fetchImportIssues).mockResolvedValue(issuePage)
        const { result } = renderHookWithProviders(() =>
            useImportIssuesQuery(query),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(issuePage)
        expect(fetchImportIssues).toHaveBeenCalledWith(query)
    })
})

describe("useImportConfirmationOperations", () => {
    it("wires completeConfirmation to the api", async () => {
        vi.mocked(completeImportConfirmation).mockResolvedValue(undefined)
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useImportConfirmationOperations(),
            { queryClient: client },
        )
        const input = {
            batchId: "b1",
            batchVersion: "1",
            trialVersion: "3",
            confirmationScope: "SALES",
            workItemId: "w1",
            taskVersion: "v1",
            subjectVersion: "s1",
            action: "CONFIRM_SCOPE" as const,
            idempotencyKey: "k2",
        }

        await act(async () => {
            await result.current.completeConfirmation(input)
        })

        expect(completeImportConfirmation).toHaveBeenCalledWith(input)
    })

    it("exposes the mutation error and clears it via resetError", async () => {
        vi.mocked(completeImportConfirmation).mockRejectedValue(
            new Error("conflict"),
        )
        const { result } = renderHookWithProviders(() =>
            useImportConfirmationOperations(),
        )

        await act(async () => {
            await expect(
                result.current.completeConfirmation({
                    batchId: "b1",
                    batchVersion: "1",
                    trialVersion: "3",
                    confirmationScope: "SALES",
                    workItemId: "w1",
                    taskVersion: "v1",
                    subjectVersion: "s1",
                    action: "CONFIRM_SCOPE",
                    idempotencyKey: "k1",
                }),
            ).rejects.toThrow("conflict")
        })
        await waitFor(() =>
            expect(result.current.error).toEqual(new Error("conflict")),
        )

        act(() => result.current.resetError())
        await waitFor(() => expect(result.current.error).toBeNull())
    })
})

describe("useImportExecutionOperations", () => {
    it("executes a command and invalidates detail/list/issues", async () => {
        vi.mocked(executeImportCommand).mockResolvedValue(execResult)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useImportExecutionOperations(),
            { queryClient: client },
        )
        const input = {
            batchId: "b1",
            expectedBatchVersion: "1",
            expectedTrialVersion: "3",
            action: "START_APPLY" as const,
            requestId: "r1",
        }

        await act(async () => {
            const out = await result.current.execute(input)
            expect(out).toEqual(execResult)
        })

        expect(executeImportCommand).toHaveBeenCalledWith(input)
        await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(3))
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: [...importOpeningKeys.all, "detail"],
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: [...importOpeningKeys.all, "list"],
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: [...importOpeningKeys.all, "issues"],
        })
    })
})
