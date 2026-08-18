import { describe, it, expect, vi, beforeEach } from "vitest"
import { act } from "@testing-library/react"

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: vi.fn(),
    }),
}))

const queryMocks = vi.hoisted(() => ({
    submitDecision: vi.fn(),
}))

vi.mock("@/features/sales-orders/hooks/queries", () => ({
    useSalesChangeReviewDecisionMutation: vi.fn(() => ({
        mutateAsync: queryMocks.submitDecision,
        isPending: false,
    })),
}))

const workItemMocks = vi.hoisted(() => ({
    refetch: vi.fn(),
    responsibility: vi.fn(),
    useWorkItemDetailQuery: vi.fn(),
    mapWorkItemDto: vi.fn(),
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemDetailQuery: workItemMocks.useWorkItemDetailQuery,
    useWorkItemResponsibilityMutation: vi.fn(() => ({
        mutateAsync: workItemMocks.responsibility,
        isPending: false,
    })),
    mapWorkItemDto: workItemMocks.mapWorkItemDto,
}))

import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"
import { useSalesChangeReviewActions } from "@/features/sales-orders/hooks/use-change-review-actions"
import { renderHookWithProviders } from "@/features/test-utils"

type Projection = {
    workItemId: string
    handlerKey: string
    status: "OPEN" | "COMPLETED" | "CLOSED"
    processingState: string
    businessObjectType: string
    rootBusinessObjectId: string
    taskVersion: string
    subjectVersion: string
    allowedActions: string[]
}

const makeChangeOrder = (): SalesChangeOrderSummary => ({
    id: "co-1",
    statusLabel: "待复核",
    statusTone: "warning",
    baseRevisionNo: 2,
    createdAt: "2026-08-14",
    impactPath: "procurement",
})

const makeWorkItem = (overrides: Partial<Projection> = {}): Projection => ({
    workItemId: "wi-1",
    handlerKey: "sales_change_impact_review",
    status: "OPEN",
    processingState: "READY",
    businessObjectType: "sales_change_review",
    rootBusinessObjectId: "so-1",
    taskVersion: "3",
    subjectVersion: "sv-1",
    allowedActions: ["PROCESS", "START_PROCESSING", "RELEASE_TO_TEAM"],
    ...overrides,
})

describe("useSalesChangeReviewActions", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        queryMocks.submitDecision.mockReset()
        workItemMocks.responsibility.mockReset()
        workItemMocks.refetch.mockReset()
        workItemMocks.mapWorkItemDto.mockImplementation(
            (dto: Projection) => dto,
        )
        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: makeWorkItem(),
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
    })

    const renderActions = (onResult = vi.fn()) => {
        const { result } = renderHookWithProviders(() =>
            useSalesChangeReviewActions({
                salesOrderId: "so-1",
                changeOrder: makeChangeOrder(),
                workItemId: "wi-1",
                returnTo: "/tasks",
                onResult,
            }),
        )
        return { result, onResult }
    }

    it("derives a valid task face for a matching open work item", () => {
        const { result } = renderActions()
        expect(result.current.valid).toBe(true)
        expect(result.current.handlerMatches).toBe(true)
        expect(result.current.canProcess).toBe(true)
        expect(result.current.canStart).toBe(true)
        expect(result.current.canRelease).toBe(true)
        expect(result.current.responsibilityStatus).toBe("pool_available")
    })

    it("flags the face invalid when the work item is missing", () => {
        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: null,
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
        const { result } = renderActions()
        expect(result.current.valid).toBe(false)
        expect(result.current.workItem).toBeNull()
        expect(result.current.responsibilityStatus).toBe("blocked")
    })

    it("flags the face invalid when the handler or business object mismatches", () => {
        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: makeWorkItem({ handlerKey: "other_handler" }),
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
        const { result } = renderActions()
        expect(result.current.handlerMatches).toBe(false)
        expect(result.current.valid).toBe(false)
    })

    it("derives terminal responsibility statuses", () => {
        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: makeWorkItem({ status: "COMPLETED", allowedActions: [] }),
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
        const { result } = renderActions()
        expect(result.current.responsibilityStatus).toBe("completed")

        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: makeWorkItem({ status: "CLOSED", allowedActions: [] }),
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
        const closed = renderHookWithProviders(() =>
            useSalesChangeReviewActions({
                salesOrderId: "so-1",
                changeOrder: makeChangeOrder(),
                workItemId: "wi-1",
                returnTo: "/tasks",
                onResult: vi.fn(),
            }),
        )
        expect(closed.result.current.responsibilityStatus).toBe("closed")
        expect(result.current.valid).toBe(false)
    })

    it("marks a processable direct assignment as assigned to me", () => {
        workItemMocks.useWorkItemDetailQuery.mockReturnValue({
            data: makeWorkItem({ allowedActions: ["PROCESS"] }),
            isLoading: false,
            refetch: workItemMocks.refetch,
        })
        const { result } = renderActions()
        expect(result.current.canProcess).toBe(true)
        expect(result.current.canStart).toBe(false)
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
    })

    it("submits an approve decision with a frozen command key", async () => {
        queryMocks.submitDecision.mockResolvedValue({ id: "co-1" })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.submitDecision("APPROVE")
        })

        const payload = queryMocks.submitDecision.mock.calls[0][0]
        expect(payload).toMatchObject({
            salesChangeOrderId: "co-1",
            handlerKey: "sales_change_impact_review",
            decision: "APPROVE",
            workItemId: "wi-1",
            expectedTaskVersion: "3",
            expectedSubjectVersion: "sv-1",
            decisionReason: undefined,
        })
        expect(payload.idempotencyKey).toMatch(
            /^w05-change-review:wi-1:APPROVE:/,
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "销售变更复核已通过",
            description: "已形成财务复核任务。",
            reference: "co-1",
            nextResponsible: "财务",
        })
    })

    it("requires a reason for reject decisions", async () => {
        const { result, onResult } = renderActions()

        await act(async () => {
            await expect(
                result.current.submitDecision("REJECT"),
            ).rejects.toThrow("驳回原因不能为空")
        })

        expect(queryMocks.submitDecision).not.toHaveBeenCalled()
        expect(onResult).not.toHaveBeenCalled()
    })

    it("reuses the same command key after an uncertain failure", async () => {
        const networkFailure = { kind: "Network", message: "网络中断" }
        queryMocks.submitDecision
            .mockRejectedValueOnce(networkFailure)
            .mockResolvedValueOnce({ id: "co-1" })
        const { result } = renderActions()

        act(() => {
            result.current.setReason("影响范围已核对")
        })

        await act(async () => {
            await expect(
                result.current.submitDecision("REJECT"),
            ).rejects.toEqual(networkFailure)
        })
        await act(async () => {
            await result.current.submitDecision("REJECT")
        })

        const first = queryMocks.submitDecision.mock.calls[0][0]
        const second = queryMocks.submitDecision.mock.calls[1][0]
        expect(second.idempotencyKey).toBe(first.idempotencyKey)
        expect(second.decisionReason).toBe("影响范围已核对")
    })

    it("mints a fresh command key after a settled success", async () => {
        queryMocks.submitDecision
            .mockResolvedValueOnce({ id: "co-1" })
            .mockResolvedValueOnce({ id: "co-1" })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.submitDecision("APPROVE")
        })
        await act(async () => {
            await result.current.submitDecision("APPROVE")
        })

        const first = queryMocks.submitDecision.mock.calls[0][0]
        const second = queryMocks.submitDecision.mock.calls[1][0]
        expect(second.idempotencyKey).not.toBe(first.idempotencyKey)
        expect(onResult).toHaveBeenCalledTimes(2)
    })

    it("starts processing and refetches the task", async () => {
        workItemMocks.responsibility.mockResolvedValue({})
        const { result } = renderActions()

        await act(async () => {
            await result.current.startProcessing()
        })

        expect(workItemMocks.responsibility).toHaveBeenCalledWith({
            kind: "START_PROCESSING",
            workItemId: "wi-1",
            expectedTaskVersion: "3",
            idempotencyKey: "w05:wi-1:3:START",
        })
        expect(workItemMocks.refetch).toHaveBeenCalledTimes(1)
    })

    it("releases the task back to the team and refetches", async () => {
        workItemMocks.responsibility.mockResolvedValue({})
        const { result } = renderActions()

        await act(async () => {
            await result.current.releaseToTeam()
        })

        expect(workItemMocks.responsibility).toHaveBeenCalledWith({
            kind: "RELEASE_TO_TEAM",
            workItemId: "wi-1",
            expectedTaskVersion: "3",
            reason: "当前处理人退回责任池",
            idempotencyKey: "w05:wi-1:3:RELEASE",
        })
        expect(workItemMocks.refetch).toHaveBeenCalledTimes(1)
    })

    it("exposes the router for the back navigation", () => {
        const { result } = renderActions()
        result.current.router.push("/tasks")
        expect(navMocks.push).toHaveBeenCalledWith("/tasks")
    })
})
