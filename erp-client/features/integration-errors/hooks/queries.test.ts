import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import * as api from "../api/requests"
import { makeItem, makeQueueView, makeResult } from "../pages/hooks/test-fixtures"
import type {
    DirectReconciliationInput,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
} from "../types"
import {
    useDirectReconciliationMutation,
    useIntegrationActionMutation,
    useIntegrationItemQuery,
    useIntegrationQueueQuery,
    useResolveIntegrationMutation,
} from "./queries"

vi.mock("../api/requests", () => ({
    fetchIntegrationQueue: vi.fn(),
    fetchIntegrationItem: vi.fn(),
    applyIntegrationTaskAction: vi.fn(),
    resolveIntegrationTask: vi.fn(),
    applyDirectReconciliation: vi.fn(),
}))

const makeQuery = () => ({
    view: "mine" as const,
    mode: "all" as const,
    environment: "production" as const,
    owner: "me" as const,
})

const taskActionInput: IntegrationTaskActionInput = {
    itemType: "ERROR_TASK",
    itemId: "task-1",
    workItemId: "wi_1",
    expectedSubjectVersion: "v3",
    expectedTaskVersion: "5",
    kind: "QUERY_ORIGINAL_RESULT",
    operationId: "op-1",
    idempotencyKey: "ik-1",
}

const resolveInput: IntegrationResolveInput = {
    itemType: "ERROR_TASK",
    itemId: "task-1",
    workItemId: "wi_1",
    expectedSubjectVersion: "v3",
    expectedTaskVersion: "5",
    operationId: "op-1",
    idempotencyKey: "ik-1",
    reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
    evidencePolicyId: "pol1",
    evidencePolicyVersion: 2,
    policyKey: { errorType: "W29", fundsImpact: "NONE" },
    evidenceRefs: [],
}

const directInput: DirectReconciliationInput = {
    differenceId: "diff-1",
    expectedDifferenceVersion: "v1",
    operationId: "op-1",
    idempotencyKey: "ik-1",
    decision: { kind: "NON_TERMINAL_ACTION", action: "ADD_EVIDENCE" },
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useIntegrationQueueQuery", () => {
    it("fetches the queue with the query and reuses a stable key", async () => {
        vi.mocked(api.fetchIntegrationQueue).mockResolvedValue(makeQueueView())
        const client = createFreshQueryClient()
        const query = makeQuery()
        const { result, rerender } = renderHookWithProviders(
            () => useIntegrationQueueQuery(query),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(api.fetchIntegrationQueue).toHaveBeenCalledWith(query)
        expect(result.current.data?.items).toHaveLength(1)

        rerender()
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        const keys = client
            .getQueryCache()
            .findAll()
            .filter((entry) => Array.isArray(entry.queryKey) && entry.queryKey[0] === "integration-errors")
            .map((entry) => entry.queryKey)
        expect(keys).toHaveLength(1)
        expect(api.fetchIntegrationQueue).toHaveBeenCalledTimes(1)
    })

    it("surfaces errors without retrying", async () => {
        vi.mocked(api.fetchIntegrationQueue).mockRejectedValue(
            new Error("服务不可用"),
        )
        const { result } = renderHookWithProviders(
            () => useIntegrationQueueQuery(makeQuery()),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual(new Error("服务不可用"))
    })
})

describe("useIntegrationItemQuery", () => {
    it("fetches the item with the given type and id", async () => {
        vi.mocked(api.fetchIntegrationItem).mockResolvedValue(makeItem())
        const { result } = renderHookWithProviders(() =>
            useIntegrationItemQuery({
                itemType: "RECONCILIATION_DIFFERENCE",
                id: "diff-1",
            }),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(api.fetchIntegrationItem).toHaveBeenCalledWith({
            itemType: "RECONCILIATION_DIFFERENCE",
            id: "diff-1",
        })
    })

    it("skips fetching when disabled or the id is empty", () => {
        const disabled = renderHookWithProviders(() =>
            useIntegrationItemQuery({
                itemType: "ERROR_TASK",
                id: "t1",
                enabled: false,
            }),
        )
        expect(disabled.result.current.fetchStatus).toBe("idle")

        const empty = renderHookWithProviders(() =>
            useIntegrationItemQuery({ itemType: "ERROR_TASK", id: "" }),
        )
        expect(empty.result.current.fetchStatus).toBe("idle")
        expect(api.fetchIntegrationItem).not.toHaveBeenCalled()
    })
})

describe("useIntegrationActionMutation", () => {
    it("calls the api and invalidates both caches on success", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.applyIntegrationTaskAction).mockResolvedValue(
            makeResult({ status: "succeeded" }),
        )
        const { result } = renderHookWithProviders(
            () => useIntegrationActionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(taskActionInput)
        })

        expect(api.applyIntegrationTaskAction).toHaveBeenCalledWith(
            taskActionInput,
        )
        expect(invalidateSpy.mock.calls.map((call) => call[0])).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ queryKey: ["integration-errors"] }),
                expect.objectContaining({ queryKey: ["work-items"] }),
            ]),
        )
    })

    it("also invalidates on an unknown result", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.applyIntegrationTaskAction).mockResolvedValue(
            makeResult({ status: "unknown" }),
        )
        const { result } = renderHookWithProviders(
            () => useIntegrationActionMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(taskActionInput)
        })
        expect(invalidateSpy).toHaveBeenCalled()
    })

    it("does not invalidate for blocked results", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.applyIntegrationTaskAction).mockResolvedValue(
            makeResult({ status: "blocked" }),
        )
        const { result } = renderHookWithProviders(
            () => useIntegrationActionMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(taskActionInput)
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe("useResolveIntegrationMutation", () => {
    it("calls the api and invalidates both caches on success", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.resolveIntegrationTask).mockResolvedValue(
            makeResult({ status: "succeeded", terminal: true }),
        )
        const { result } = renderHookWithProviders(
            () => useResolveIntegrationMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(resolveInput)
        })
        expect(api.resolveIntegrationTask).toHaveBeenCalledWith(resolveInput)
        expect(invalidateSpy.mock.calls.map((call) => call[0])).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ queryKey: ["integration-errors"] }),
                expect.objectContaining({ queryKey: ["work-items"] }),
            ]),
        )
    })
})

describe("useDirectReconciliationMutation", () => {
    it("invalidates only integration errors on success", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.applyDirectReconciliation).mockResolvedValue(
            makeResult({ status: "succeeded" }),
        )
        const { result } = renderHookWithProviders(
            () => useDirectReconciliationMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(directInput)
        })
        expect(api.applyDirectReconciliation).toHaveBeenCalledWith(directInput)
        const keys = invalidateSpy.mock.calls.map((call) => call[0])
        expect(keys).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ queryKey: ["integration-errors"] }),
            ]),
        )
        expect(
            keys.some(
                (arg) =>
                    arg &&
                    typeof arg === "object" &&
                    "queryKey" in arg &&
                    Array.isArray(arg.queryKey) &&
                    arg.queryKey[0] === "work-items",
            ),
        ).toBe(false)
    })

    it("does not invalidate for unknown results", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        vi.mocked(api.applyDirectReconciliation).mockResolvedValue(
            makeResult({ status: "unknown" }),
        )
        const { result } = renderHookWithProviders(
            () => useDirectReconciliationMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(directInput)
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
