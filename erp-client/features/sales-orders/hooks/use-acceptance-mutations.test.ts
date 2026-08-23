import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import * as api from "@/features/sales-orders/api/acceptance"
import { useAcceptanceMutations } from "./use-acceptance-mutations"
import { salesOrderKeys } from "./queries"
import { resultText } from "@/lib/ui-text"
import type {
    AcceptanceOverallResult,
    PostAcceptanceInput,
    ReverseAcceptanceInput,
    SaveAcceptanceDraftInput,
} from "@/features/sales-orders/lib/acceptance-types"

vi.mock("@/features/sales-orders/api/acceptance", () => ({
    fetchCustomerAcceptanceWorkspace: vi.fn(),
    saveCustomerAcceptanceDraft: vi.fn(),
    postCustomerAcceptanceWorkspace: vi.fn(),
    reverseCustomerAcceptanceWorkspace: vi.fn(),
}))

const draftInput: SaveAcceptanceDraftInput = {
    salesOrderId: "so_1",
    acceptanceDraftId: "draft_1",
    expectedDraftVersion: 1,
    acceptedAt: "2026-08-01T00:00:00.000Z",
    comment: "内部备注",
    lines: [],
}

const postInput: PostAcceptanceInput = {
    salesOrderId: "so_1",
    acceptanceDraftId: "draft_1",
    expectedDraftVersion: 1,
    expectedSalesOrderLockVersion: 3,
    idempotencyKey: "acc-key-1",
    acceptedAt: "2026-08-01T00:00:00.000Z",
    comment: "",
    lines: [],
}

const reverseInput: ReverseAcceptanceInput = {
    salesOrderId: "so_1",
    acceptanceId: "a1",
    expectedAcceptanceVersion: 1,
    reasonText: "误录",
    idempotencyKey: "rev-1",
}

function renderMutations(submittedOverall: AcceptanceOverallResult = "PASS") {
    const client = createFreshQueryClient()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const setDraftSavedAt = vi.fn()
    const onPostSucceeded = vi.fn()
    const onReverseSucceeded = vi.fn()
    const submittedOverallRef = { current: submittedOverall }
    const hook = renderHookWithProviders(
        () =>
            useAcceptanceMutations({
                salesOrderId: "so_1",
                idempotencyKey: "acc-key-1",
                submittedOverallRef,
                setDraftSavedAt,
                onPostSucceeded,
                onReverseSucceeded,
            }),
        { queryClient: client },
    )
    return {
        hook,
        invalidateSpy,
        setDraftSavedAt,
        onPostSucceeded,
        onReverseSucceeded,
        submittedOverallRef,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useAcceptanceMutations", () => {
    it("wires saveDraft to the api and records the saved time", async () => {
        vi.mocked(api.saveCustomerAcceptanceDraft).mockResolvedValue({
            acceptanceDraftId: "draft_2",
            draftVersion: 2,
            salesOrderId: "so_1",
            acceptedAt: "2026-08-01T00:00:00.000Z",
            comment: "内部备注",
            lines: [],
            updatedAt: "2026-08-01T01:00:00.000Z",
        })
        const { hook, setDraftSavedAt, invalidateSpy } = renderMutations()

        await act(async () => {
            await hook.result.current.saveDraftMutation.mutateAsync(draftInput)
        })

        expect(
            vi.mocked(api.saveCustomerAcceptanceDraft).mock.calls[0]?.[0],
        ).toEqual(draftInput)
        expect(setDraftSavedAt).toHaveBeenCalledWith("2026-08-01T01:00:00.000Z")
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.acceptanceRoot("so_1"),
        })
    })

    it("keeps draft time untouched when saving the draft fails", async () => {
        vi.mocked(api.saveCustomerAcceptanceDraft).mockRejectedValue(
            new Error("save failed"),
        )
        const { hook, setDraftSavedAt } = renderMutations()

        await act(async () => {
            await hook.result.current.saveDraftMutation
                .mutateAsync(draftInput)
                .catch(() => {})
        })

        expect(setDraftSavedAt).not.toHaveBeenCalled()
        await waitFor(() =>
            expect(hook.result.current.saveDraftMutation.isError).toBe(true),
        )
    })

    it("registers acceptance successfully and resets via onPostSucceeded", async () => {
        vi.mocked(api.postCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "succeeded",
            acceptanceNo: "YS-1",
            acceptanceId: "a1",
            remainingEligibleCount: 3,
            remainingEligibleQuantityLabel: "约 12 件",
            overallResult: "PASS",
            factOnlyNotice: "本结果仅记录客户验收记录",
        })
        const { hook, onPostSucceeded, invalidateSpy } = renderMutations()

        await act(async () => {
            await hook.result.current.postMutation.mutateAsync(postInput)
        })

        expect(
            vi.mocked(api.postCustomerAcceptanceWorkspace).mock.calls[0]?.[0],
        ).toEqual(postInput)
        expect(onPostSucceeded).toHaveBeenCalledTimes(1)
        expect(hook.result.current.formalResult).toMatchObject({
            kind: "post",
            status: "succeeded",
            title: "客户验收已登记",
            reference: "YS-1",
            facts: expect.arrayContaining([
                {
                    label: "履约轨",
                    value: "仍待验收 3 批 · 约 12 件",
                },
            ]),
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.acceptanceRoot("so_1"),
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so_1"),
        })
    })

    it("uses the submitted overall snapshot in the success feedback", async () => {
        vi.mocked(api.postCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "succeeded",
            acceptanceNo: "YS-2",
            acceptanceId: "a2",
            remainingEligibleCount: 0,
            remainingEligibleQuantityLabel: "",
            overallResult: "PASS",
            factOnlyNotice: "本结果仅记录客户验收记录",
        })
        const { hook } = renderMutations("SHORT")

        await act(async () => {
            await hook.result.current.postMutation.mutateAsync(postInput)
        })

        const facts = hook.result.current.formalResult?.facts ?? []
        expect(facts[0]).toEqual({ label: "总体结果", value: "短少" })
        expect(facts[2]).toEqual({
            label: "履约轨",
            value: "待验收已清零",
        })
        expect(facts[3]).toEqual({
            label: "下一步",
            value: "销售协同处理验收异常",
        })
    })

    it("surfaces an unknown post result without resetting the workspace", async () => {
        vi.mocked(api.postCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "unknown",
            message: "操作结果暂无法确认",
            idempotencyKey: "acc-key-1",
        })
        const { hook, onPostSucceeded } = renderMutations()

        await act(async () => {
            await hook.result.current.postMutation.mutateAsync(postInput)
        })

        expect(onPostSucceeded).not.toHaveBeenCalled()
        expect(hook.result.current.formalResult).toMatchObject({
            kind: "post",
            status: "unknown",
            title: resultText.unknown,
            facts: [{ label: resultText.originalTaskNo, value: "acc-key-1" }],
        })
    })

    it("surfaces a failed post result", async () => {
        vi.mocked(api.postCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "failed",
            message: "验收过账失败",
        })
        const { hook } = renderMutations()

        await act(async () => {
            await hook.result.current.postMutation.mutateAsync(postInput)
        })

        expect(hook.result.current.formalResult).toMatchObject({
            kind: "post",
            status: "failed",
            title: "验收登记失败",
            description: "验收过账失败",
        })
    })

    it("treats a thrown post error as an unknown result with the idempotency key", async () => {
        vi.mocked(api.postCustomerAcceptanceWorkspace).mockRejectedValue(
            new Error("timeout"),
        )
        const { hook } = renderMutations()

        await act(async () => {
            await hook.result.current.postMutation
                .mutateAsync(postInput)
                .catch(() => {})
        })

        expect(hook.result.current.formalResult).toMatchObject({
            kind: "post",
            status: "unknown",
            facts: [{ label: resultText.originalTaskNo, value: "acc-key-1" }],
        })
    })

    it("reverses an acceptance and reports the reference numbers", async () => {
        vi.mocked(api.reverseCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "succeeded",
            reverseAcceptanceNo: "YS-R1",
            reverseAcceptanceId: "r1",
            originalAcceptanceNo: "YS-1",
        })
        const { hook, onReverseSucceeded } = renderMutations()

        await act(async () => {
            await hook.result.current.reverseMutation.mutateAsync(reverseInput)
        })

        expect(
            vi.mocked(api.reverseCustomerAcceptanceWorkspace).mock
                .calls[0]?.[0],
        ).toEqual(reverseInput)
        expect(onReverseSucceeded).toHaveBeenCalledTimes(1)
        expect(hook.result.current.formalResult).toMatchObject({
            kind: "reverse",
            status: "succeeded",
            title: "误录验收已冲正",
            reference: "YS-R1",
            facts: [
                { label: "原验收单号", value: "YS-1" },
                { label: "冲正单号", value: "YS-R1" },
            ],
        })
    })

    it("surfaces a failed reversal without closing the dialog", async () => {
        vi.mocked(api.reverseCustomerAcceptanceWorkspace).mockResolvedValue({
            status: "failed",
            message: "冲正失败",
        })
        const { hook, onReverseSucceeded } = renderMutations()

        await act(async () => {
            await hook.result.current.reverseMutation.mutateAsync(reverseInput)
        })

        expect(onReverseSucceeded).not.toHaveBeenCalled()
        expect(hook.result.current.formalResult).toMatchObject({
            kind: "reverse",
            status: "failed",
            title: "冲正失败",
        })
    })
})
