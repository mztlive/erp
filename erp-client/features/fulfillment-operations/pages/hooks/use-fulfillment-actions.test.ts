import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"

vi.mock("@/features/fulfillment-operations/api", () => ({
    fetchFulfillmentQueue: vi.fn(),
    saveFulfillmentOperation: vi.fn(),
    postFulfillmentOperation: vi.fn(),
    resolveUnknownFulfillmentResult: vi.fn(),
}))

import type {
    SaveFulfillmentOperationCommand,
    PostFulfillmentOperationCommand,
    ResolveFulfillmentOperationCommand,
    FormalActionResponse,
} from "@/features/fulfillment-operations/types"
import { useFulfillmentActions, type FulfillmentActionsOptions } from "./use-fulfillment-actions"
import { makeOperation, makePostedOutcome } from "./test-data"

type SaveMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").useSaveFulfillmentMutation
>
type PostMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").usePostFulfillmentMutation
>
type ResolveUnknownMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").useResolveUnknownFulfillmentMutation
>

type SaveMutationMock = SaveMutation & {
    mutateAsync: ReturnType<typeof vi.fn>
}
type PostMutationMock = PostMutation & {
    mutateAsync: ReturnType<typeof vi.fn>
}
type ResolveUnknownMutationMock = ResolveUnknownMutation & {
    mutateAsync: ReturnType<typeof vi.fn>
}

function makeSaveMutation(): SaveMutationMock {
    return {
        mutateAsync: vi.fn(
            async (_cmd: SaveFulfillmentOperationCommand) => ({
                editVersion: 4,
            }),
        ),
        // 编排只消费 mutateAsync；其余成员以断言补齐
    } as unknown as SaveMutationMock
}

function makePostMutation(): PostMutationMock {
    return {
        mutateAsync: vi.fn(
            async (_cmd: PostFulfillmentOperationCommand) =>
                ({
                    status: "succeeded",
                    outcome: makePostedOutcome(),
                }) satisfies FormalActionResponse,
        ),
    } as unknown as PostMutationMock
}

function makeResolveMutation(): ResolveUnknownMutationMock {
    return {
        mutateAsync: vi.fn(
            async (_cmd: ResolveFulfillmentOperationCommand) =>
                ({
                    status: "succeeded",
                    outcome: makePostedOutcome(),
                }) satisfies FormalActionResponse,
        ),
    } as unknown as ResolveUnknownMutationMock
}

function renderActions(overrides: {
    dirty?: boolean
    autoNext?: boolean
    pendingIdempotencyKey?: string
    operation?: ReturnType<typeof makeOperation>
    draft?: ReturnType<typeof makeOperation>["draft"] | null
} = {}) {
    const operation = overrides.operation ?? makeOperation()
    const draft = overrides.draft ?? operation.draft
    const state = {
        dirty: overrides.dirty ?? false,
        actionError: null as string | null,
        saveMessage: null as string | null,
        confirmOpen: false,
        lastResult: null as unknown,
    }
    const callbacks = {
        neighborId: vi.fn((delta: number) =>
            delta === 1 ? "op_2" : undefined,
        ),
        goToOperation: vi.fn(),
        advanceIfNeeded: vi.fn(),
    }
    const setters = {
        setDirty: vi.fn((next: boolean) => {
            state.dirty = next
        }),
        setActionError: vi.fn((next: string | null) => {
            state.actionError = next
        }),
        setSaveMessage: vi.fn((next: string | null) => {
            state.saveMessage = next
        }),
        setConfirmOpen: vi.fn((next: boolean) => {
            state.confirmOpen = next
        }),
        setLastResult: vi.fn((next: unknown) => {
            state.lastResult = next
        }),
    }
    const mutations = {
        saveMutation: makeSaveMutation(),
        postMutation: makePostMutation(),
        resolveUnknownMutation: makeResolveMutation(),
    }
    const utils = renderHook((props: typeof overrides) => {
        const current = props.operation ?? operation
        return useFulfillmentActions({
            operation: current,
            draft: props.draft ?? draft,
            dirty: props.dirty ?? false,
            autoNext: props.autoNext ?? true,
            pendingIdempotencyKey: props.pendingIdempotencyKey,
            ...mutations,
            neighborId: callbacks.neighborId,
            goToOperation: callbacks.goToOperation,
            advanceIfNeeded: callbacks.advanceIfNeeded,
            ...setters,
        } as FulfillmentActionsOptions)
    }, { initialProps: overrides })
    return { ...utils, state, callbacks, setters, mutations }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useFulfillmentActions", () => {
    it("exposes supportsSave only for draftable types", () => {
        const { result } = renderActions()
        expect(result.current.supportsSave).toBe(true)
    })

    it("saves the draft with the operation versions and marks it clean", async () => {
        const { result, state, mutations } = renderActions({ dirty: true })
        let saved: boolean | undefined
        await act(async () => {
            saved = await result.current.handleSave()
        })
        expect(saved).toBe(true)
        expect(mutations.saveMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutations.saveMutation.mutateAsync.mock.calls[0][0]).toMatchObject(
            {
                operationId: "op_1",
                expectedDocumentVersion: 3,
                expectedSourceVersion: "sv_1",
                idempotencyKey: expect.stringContaining("w09:op_1:3:save:"),
            },
        )
        expect(state.dirty).toBe(false)
        expect(state.saveMessage).toBe("草稿已保存")
        expect(state.actionError).toBeNull()
    })

    it("reports an error without saving when the draft type cannot be saved", async () => {
        const { result, state, mutations } = renderActions({
            operation: makeOperation({ operationType: "SERVICE" }),
            draft: {
                type: "SERVICE",
                startedAt: "2026-08-14T08:00:00.000Z",
                endedAt: "2026-08-14T09:00:00.000Z",
                serviceLocation: "客户现场",
                result: "SUCCESS",
                completionNote: "服务已完成",
                lines: [],
            },
        })
        let saved: boolean | undefined
        await act(async () => {
            saved = await result.current.handleSave()
        })
        expect(saved).toBe(false)
        expect(state.actionError).toBe(
            "这类履约单据没有草稿保存命令，请直接确认",
        )
        expect(mutations.saveMutation.mutateAsync).not.toHaveBeenCalled()
    })

    it("posts the draft, records the succeeded result and advances", async () => {
        const { result, state, callbacks, mutations } = renderActions()
        await act(async () => {
            await result.current.handlePost()
        })
        expect(mutations.postMutation.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                operationId: "op_1",
                idempotencyKey: expect.stringContaining("w09:op_1:3:post:"),
            }),
        )
        expect(state.confirmOpen).toBe(false)
        expect(state.lastResult).toMatchObject({
            status: "succeeded",
            title: "已入库",
        })
        expect(callbacks.advanceIfNeeded).toHaveBeenCalledWith(
            true,
            "op_2",
            true,
        )
    })

    it("keeps an unknown post result pending for later resolution", async () => {
        const { result, state, mutations } = renderActions()
        mutations.postMutation.mutateAsync.mockResolvedValueOnce({
            status: "unknown",
            message: "处理结果待确认",
            idempotencyKey: "w09:op_1:3:post:key",
        })
        await act(async () => {
            await result.current.handlePost()
        })
        expect(state.lastResult).toMatchObject({
            status: "unknown",
            stayOnItem: true,
            pendingIdempotencyKey: "w09:op_1:3:post:key",
        })
    })

    it("skips only when the draft is clean", async () => {
        const { result, state, callbacks } = renderActions({ dirty: true })
        act(() => {
            result.current.handleSkip()
        })
        expect(state.actionError).toBe(
            "有未保存修改，请先保存或放弃后再切换",
        )
        expect(callbacks.goToOperation).not.toHaveBeenCalled()
    })

    it("resolves an unknown result with the pending request id", async () => {
        const { result, mutations } = renderActions({
            pendingIdempotencyKey: "w09:op_1:3:post:key",
        })
        await act(async () => {
            await result.current.handleResolveUnknown()
        })
        expect(mutations.resolveUnknownMutation.mutateAsync).toHaveBeenCalledWith(
            {
                operationId: "op_1",
                idempotencyKey: "w09:op_1:3:post:key",
            },
        )
    })
})
