import { act } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { workItemKeys } from "@/features/work-items/queries"

import {
    inventoryKeys,
    useCancelStockAdjustmentApprovalMutation,
} from "./queries"

const cancelStockAdjustmentApprovalMock = vi.hoisted(() => vi.fn())

vi.mock("@/features/inventory/api/inventory", async (importOriginal) => {
    const original =
        await importOriginal<
            typeof import("@/features/inventory/api/inventory")
        >()
    return {
        ...original,
        cancelStockAdjustmentApproval: cancelStockAdjustmentApprovalMock,
    }
})

describe("stock adjustment cancellation cache refresh", () => {
    beforeEach(() => {
        cancelStockAdjustmentApprovalMock.mockReset()
        cancelStockAdjustmentApprovalMock.mockResolvedValue({
            stockAdjustmentId: "adjustment-1",
            status: "DRAFT",
        })
    })

    it("invalidates inventory details, approval facts, and work items", async () => {
        const queryClient = createFreshQueryClient()
        const invalidate = vi.spyOn(queryClient, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useCancelStockAdjustmentApprovalMutation(),
            { queryClient },
        )

        await act(async () => {
            await result.current.mutateAsync({
                stockAdjustmentId: "adjustment-1",
                command: {
                    expectedVersion: "3",
                    approvalProcessInstanceId: "instance-1",
                    expectedSubjectVersion: "2",
                    expectedInstanceVersion: "7",
                    expectedExecutionVersion: "5",
                    expectedTaskVersion: "4",
                },
                reason: "需要修改数量",
                idempotencyKey: "cancel-intent-1",
            })
        })

        expect(invalidate).toHaveBeenCalledWith({ queryKey: inventoryKeys.all })
        expect(invalidate).toHaveBeenCalledWith({ queryKey: approvalKeys.all })
        expect(invalidate).toHaveBeenCalledWith({ queryKey: workItemKeys.all })
    })
})
