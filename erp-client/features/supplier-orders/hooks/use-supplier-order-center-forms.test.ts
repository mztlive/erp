import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    noteSchema,
    releaseSchema,
    useSupplierOrderCenterNoteForm,
    useSupplierOrderCenterReleaseForm,
} from "./use-supplier-order-center-forms"
import { makeDetail, makeMutation } from "./use-supplier-order-center-fixtures"
import { useSupplierOrderCenterCommandIdentity } from "./use-supplier-order-center-identity"
import type { NoteInput } from "@/features/supplier-orders/types"
import type {
    WorkItemDto,
    WorkItemResponsibilityCommand,
} from "@/features/work-items/types"

beforeEach(() => {
    vi.clearAllMocks()
})

describe("noteSchema", () => {
    it("requires at least two characters of comment", () => {
        expect(noteSchema.safeParse({ comment: "" }).success).toBe(false)
        expect(noteSchema.safeParse({ comment: "x" }).success).toBe(false)
        expect(noteSchema.safeParse({ comment: "协同" }).success).toBe(true)
        expect(noteSchema.safeParse({ comment: "  协同  " }).data).toEqual({
            comment: "协同",
        })
    })
})

describe("releaseSchema", () => {
    it("requires a reason code but allows an empty comment", () => {
        expect(releaseSchema.safeParse({ reasonCode: "", comment: "" }).success).toBe(false)
        expect(
            releaseSchema.safeParse({ reasonCode: "OTHER", comment: "" })
                .success,
        ).toBe(true)
    })
})

describe("useSupplierOrderCenterNoteForm", () => {
    it("skips submission while the comment is too short", async () => {
        const setResult = vi.fn()
        const noteMutation = makeMutation<
            { status: "succeeded" | "blocked"; message: string },
            NoteInput
        >()
        const { result } = renderHook(() =>
            useSupplierOrderCenterNoteForm({
                orderId: "o1",
                detail: makeDetail(),
                noteMutation,
                setResult,
            }),
        )
        act(() => {
            result.current.setFieldValue("comment", "x")
        })
        await act(async () => {
            await result.current.handleSubmit()
        })
        expect(noteMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).not.toHaveBeenCalled()
    })

    it("submits the comment and reports success", async () => {
        const setResult = vi.fn()
        const noteMutation = makeMutation<
            { status: "succeeded" | "blocked"; message: string },
            NoteInput
        >()
        noteMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "已记录",
        })
        const { result } = renderHook(() =>
            useSupplierOrderCenterNoteForm({
                orderId: "o1",
                detail: makeDetail(),
                noteMutation,
                setResult,
            }),
        )
        act(() => {
            result.current.setFieldValue("comment", "供应商已回传单号")
        })
        await act(async () => {
            await result.current.handleSubmit()
        })
        expect(noteMutation.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                orderId: "o1",
                expectedLockVersion: 7,
                comment: "供应商已回传单号",
            }),
        )
        expect(
            (noteMutation.mutateAsync.mock.calls[0][0] as NoteInput)
                .idempotencyKey,
        ).toMatch(/^note-o1-/)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "协同说明已记录",
            }),
        )
        expect(result.current.state.values.comment).toBe("")
    })

    it("maps a blocked response to a non-written result", async () => {
        const setResult = vi.fn()
        const noteMutation = makeMutation<
            { status: "succeeded" | "blocked"; message: string },
            NoteInput
        >()
        noteMutation.mutateAsync.mockResolvedValue({
            status: "blocked",
            message: "协同说明写入端点尚未交付。",
        })
        const { result } = renderHook(() =>
            useSupplierOrderCenterNoteForm({
                orderId: "o1",
                detail: makeDetail(),
                noteMutation,
                setResult,
            }),
        )
        act(() => {
            result.current.setFieldValue("comment", "供应商已回传单号")
        })
        await act(async () => {
            await result.current.handleSubmit()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "协同说明未写入",
            }),
        )
    })

    it("does nothing while the detail is missing", async () => {
        const setResult = vi.fn()
        const noteMutation = makeMutation<
            { status: "succeeded" | "blocked"; message: string },
            NoteInput
        >()
        const { result } = renderHook(() =>
            useSupplierOrderCenterNoteForm({
                orderId: "o1",
                detail: undefined,
                noteMutation,
                setResult,
            }),
        )
        act(() => {
            result.current.setFieldValue("comment", "供应商已回传单号")
        })
        await act(async () => {
            await result.current.handleSubmit()
        })
        expect(noteMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).not.toHaveBeenCalled()
    })
})

describe("useSupplierOrderCenterReleaseForm", () => {
    function renderReleaseForm() {
        const setResult = vi.fn()
        const refetch = vi.fn()
        const responsibilityMutation = makeMutation<
            WorkItemDto,
            WorkItemResponsibilityCommand
        >()
        const forgetCommandIdentity = vi.fn()
        const identity = renderHook(() =>
            useSupplierOrderCenterCommandIdentity(),
        )
        const { result } = renderHook(() =>
            useSupplierOrderCenterReleaseForm({
                detail: makeDetail(),
                setResult,
                responsibilityMutation,
                refetch,
                commandIdentity: identity.result.current.commandIdentity,
                forgetCommandIdentity,
            }),
        )
        return {
            result,
            setResult,
            refetch,
            responsibilityMutation,
            forgetCommandIdentity,
        }
    }

    it("starts with the default reason and a closed dialog", () => {
        const { result } = renderReleaseForm()
        expect(result.current.releaseForm.state.values).toEqual({
            reasonCode: "WAITING_SUPPLIER",
            comment: "",
        })
        expect(result.current.releaseOpen).toBe(false)
    })

    it("releases to the team with the reason text", async () => {
        const {
            result,
            setResult,
            refetch,
            responsibilityMutation,
            forgetCommandIdentity,
        } = renderReleaseForm()
        responsibilityMutation.mutateAsync.mockResolvedValue({
            id: "wi1",
            work_item_type: "integration_result_unknown",
            handler_key: "supplier-orders",
            approval_step_instance_id: null,
            status: "OPEN",
            assignment_mode: "POOL",
            assignment_source: "released",
            owner_role: "ops",
            owner_organization_id: "org1",
            processing_state: "READY",
            business_object_type: "supplier_fulfillment_order",
            business_object_id: "o1",
            root_business_object_id: "o1",
            subject_version: "v2",
            task_version: "3",
            priority: "HIGH",
            created_at: 1_700_000_000_000,
        })
        act(() => {
            result.current.setReleaseOpen(true)
            result.current.releaseForm.setFieldValue("comment", "等待商城协同")
        })
        await act(async () => {
            await result.current.releaseForm.handleSubmit()
        })
        const command = responsibilityMutation.mutateAsync.mock
            .calls[0][0] as WorkItemResponsibilityCommand
        expect(command).toEqual(
            expect.objectContaining({
                kind: "RELEASE_TO_TEAM",
                workItemId: "wi1",
                expectedTaskVersion: "3",
                reason: "WAITING_SUPPLIER: 等待商城协同",
            }),
        )
        expect(command.idempotencyKey).toMatch(/^w26:release-to-team:/)
        expect(forgetCommandIdentity).toHaveBeenCalledWith(
            expect.stringContaining("release-to-team:"),
        )
        expect(result.current.releaseOpen).toBe(false)
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "任务已退回团队",
                reference: "wi1",
            }),
        )
        expect(result.current.releaseForm.state.values).toEqual({
            reasonCode: "WAITING_SUPPLIER",
            comment: "",
        })
    })

    it("sends only the reason code when the comment is blank", async () => {
        const { result, responsibilityMutation } = renderReleaseForm()
        responsibilityMutation.mutateAsync.mockResolvedValue({
            id: "wi1",
            work_item_type: "integration_result_unknown",
            handler_key: "supplier-orders",
            approval_step_instance_id: null,
            status: "OPEN",
            assignment_mode: "POOL",
            assignment_source: "released",
            owner_role: "ops",
            owner_organization_id: "org1",
            processing_state: "READY",
            business_object_type: "supplier_fulfillment_order",
            business_object_id: "o1",
            root_business_object_id: "o1",
            subject_version: "v2",
            task_version: "3",
            priority: "HIGH",
            created_at: 1_700_000_000_000,
        })
        await act(async () => {
            await result.current.releaseForm.handleSubmit()
        })
        const command = responsibilityMutation.mutateAsync.mock
            .calls[0][0] as Extract<
            WorkItemResponsibilityCommand,
            { kind: "RELEASE_TO_TEAM" }
        >
        expect(command.reason).toBe("WAITING_SUPPLIER")
    })

    it("does nothing without a work item", async () => {
        const setResult = vi.fn()
        const refetch = vi.fn()
        const responsibilityMutation = makeMutation<
            WorkItemDto,
            WorkItemResponsibilityCommand
        >()
        const { result } = renderHook(() =>
            useSupplierOrderCenterReleaseForm({
                detail: makeDetail({ workItem: undefined }),
                setResult,
                responsibilityMutation,
                refetch,
                commandIdentity: () => ({
                    key: "k",
                    operationId: "op",
                    idempotencyKey: "idem",
                }),
                forgetCommandIdentity: vi.fn(),
            }),
        )
        await act(async () => {
            await result.current.releaseForm.handleSubmit()
        })
        expect(responsibilityMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).not.toHaveBeenCalled()
    })
})
