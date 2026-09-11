import * as React from "react"
import {
    act,
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { mapCategoryRow } from "@/features/master-data/api/list-mappers"
import { CategoryReviseDialog } from "./category-form-dialogs"
import { updateCategoryRevision } from "@/features/master-data/api/mutations/category"
import { apiPut } from "@/lib/api"

const mock = vi.hoisted(() => ({
    frame: null as any,
    mutate: vi.fn(),
    close: vi.fn(),
}))
vi.mock("@/features/master-data/hooks/queries", () => ({
    useCreateRevisionMutation: () => ({
        mutateAsync: mock.mutate,
        isPending: false,
    }),
    useCreateMasterDataMutation: () => ({
        mutateAsync: vi.fn(),
        isPending: false,
    }),
}))
vi.mock(
    "@/features/master-data/components/shared/action-dialog-shared",
    () => ({ newIdempotencyKey: () => "test-key", notifySuccess: vi.fn() }),
)
vi.mock("@/lib/api", () => ({ apiPut: vi.fn(), apiPost: vi.fn() }))
// Keep the real TanStack Form and mutation adapter; only replace the visual frame.
vi.mock("./category-form-dialog-frame", () => ({
    CategoryFormDialogFrame: (props: any) => {
        mock.frame = props
        return (
            <button onClick={() => props.form.handleSubmit()}>提交测试</button>
        )
    },
}))
const target = mapCategoryRow({
    id: "tea",
    category_code: "TEA",
    name: "茶",
    parent_category_id: "food",
    product_kind: "PHYSICAL",
    status: "active",
    version: 3,
    created_at: 1,
})
beforeEach(() => {
    mock.mutate.mockReset()
    mock.close.mockReset()
    vi.mocked(apiPut).mockReset()
})
afterEach(cleanup)

describe("category edit form contract", () => {
    it("fills an existing category without treating it as unsaved changes", async () => {
        const { rerender } = render(
            <CategoryReviseDialog
                open
                target={target}
                onOpenChange={mock.close}
            />,
        )
        await waitFor(() =>
            expect(mock.frame.form.state.values.name).toBe("茶"),
        )
        expect(mock.frame.form.state.values.code).toBe("TEA")
        expect(mock.frame.form.state.isDirty).toBe(false)
        rerender(
            <CategoryReviseDialog
                open
                target={target}
                onOpenChange={mock.close}
            />,
        )
        expect(mock.frame.form.state.values.name).toBe("茶")
        act(() => mock.frame.form.setFieldValue("name", "新名称"))
        expect(mock.frame.form.state.isDirty).toBe(true)
    })
    it("sends an explicit root move through the real adapter, preserving the category code and lock", async () => {
        vi.mocked(apiPut).mockResolvedValue({
            id: "tea",
            category_code: "TEA",
            version: 4,
        })
        mock.mutate.mockImplementation(updateCategoryRevision)
        render(
            <CategoryReviseDialog
                mode="move"
                open
                target={target}
                onOpenChange={mock.close}
            />,
        )
        await waitFor(() =>
            expect(mock.frame.form.state.values.parentId).toBe("food"),
        )
        act(() => {
            mock.frame.form.setFieldValue("parentId", "")
            mock.frame.form.setFieldValue("changeReason", "调整为一级分类")
        })
        fireEvent.click(screen.getByText("提交测试"))
        await waitFor(() => expect(apiPut).toHaveBeenCalled())
        expect(apiPut).toHaveBeenCalledWith(
            "/admin/product-categories/tea",
            expect.objectContaining({
                version: 3,
                name: "茶",
                parent_change: { parent_category_id: null },
            }),
        )
        expect(vi.mocked(apiPut).mock.calls[0][1]).not.toHaveProperty(
            "category_code",
        )
        await waitFor(() => expect(mock.close).toHaveBeenCalledWith(false))
    })
    it("keeps the editor open and surfaces a conflict instead of reporting success", async () => {
        mock.mutate.mockResolvedValue({
            outcome: "conflict",
            message: "分类已被其他人修改",
        })
        render(
            <CategoryReviseDialog
                open
                target={target}
                onOpenChange={mock.close}
            />,
        )
        act(() => mock.frame.form.setFieldValue("changeReason", "调整名称"))
        fireEvent.click(screen.getByText("提交测试"))
        await waitFor(() => expect(mock.frame.result?.outcome).toBe("conflict"))
        expect(mock.close).not.toHaveBeenCalled()
        expect(mock.frame.form.state.values.code).toBe("TEA")
    })
})
