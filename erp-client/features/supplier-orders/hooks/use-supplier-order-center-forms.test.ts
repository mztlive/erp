import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    noteSchema,
    useSupplierOrderCenterNoteForm,
} from "./use-supplier-order-center-forms"
import { makeDetail, makeMutation } from "./use-supplier-order-center-fixtures"
import type { NoteInput } from "@/features/supplier-orders/types"

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
