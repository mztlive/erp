import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { emptyProductFields } from "@/features/master-data/lib/product-model"
import { useProductEditor } from "./use-product-editor"
import { createSpecDraft } from "../lib/product-editor-model"
import { createProductFormBindings } from "../lib/product-form-bindings"

const mocks = vi.hoisted(() => ({
    revise: vi.fn(),
    create: vi.fn(),
    refetch: vi.fn(),
    push: vi.fn(),
    replace: vi.fn(),
    data: {} as Record<string, unknown>,
    permissions: ["product:update"],
}))
vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: mocks.push, replace: mocks.replace }),
}))
vi.mock("@/components/ui/toast", () => ({ toast: { add: vi.fn() } }))
vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: mocks.permissions },
    }),
}))
vi.mock("@/hooks/use-options", () => ({
    useUnitOptionsQuery: () => ({ data: [] }),
}))
vi.mock("./queries", () => ({
    useMasterDataCenterQuery: () => ({
        data: mocks.data,
        refetch: mocks.refetch,
    }),
    useMasterDataListQuery: () => ({ data: undefined }),
    useSkuSupplierCountsQuery: () => ({ data: undefined }),
    useCreateMasterDataMutation: () => ({
        mutateAsync: mocks.create,
        isPending: false,
    }),
    useCreateRevisionMutation: () => ({
        mutateAsync: mocks.revise,
        isPending: false,
    }),
}))

afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
})
beforeEach(() => {
    vi.clearAllMocks()
    window.history.replaceState(null, "", "/master-data/products/product-1")
    mocks.permissions = ["product:update"]
    const fields = emptyProductFields()
    Object.assign(fields, {
        productNo: "P-1",
        baseUnitId: "unit",
        baseUnitCode: "BOX",
        baseUnit: "盒",
        categoryId: "category",
        category: "茶叶",
        brandId: "brand",
        brand: "狮峰",
    })
    Object.assign(fields.skus[0], {
        skuId: "sku-1",
        name: "礼盒",
        mainImage: "tea.png",
        salePrice: "568.00",
    })
    mocks.data = {
        stableId: "product-1",
        name: "礼盒",
        lockVersion: 7,
        productKind: "PHYSICAL",
        productDetail: fields,
        currentRevision: {
            revisionId: "revision-1",
            revisionNo: 1,
            effectiveFrom: "2026-08-30",
        },
        allowedActions: ["CREATE_REVISION"],
        actionBlockers: [],
    }
    mocks.refetch.mockResolvedValue({ data: mocks.data })
})

describe("product save confirmation", () => {
    function setupSpecs() {
        const { result } = renderHook(() => useProductEditor("product-1"))
        const confirm = vi.fn()
        const offerUndo = vi.fn()
        const bindings = () =>
            createProductFormBindings(
                result.current.form,
                result.current.form.state.values,
                false,
                "礼盒",
                confirm,
                offerUndo,
            )
        return { result, bindings, confirm, offerUndo }
    }
    it("preserves SKU edits while typing, reverting and cancelling specification drafts", () => {
        const { result, bindings } = setupSpecs()
        act(() =>
            bindings().updateSku(0, {
                salePrice: "612.0001",
                barcode: "CUSTOM",
            }),
        )
        const before = structuredClone(
            result.current.form.state.values.fields.skus,
        )
        act(() =>
            bindings().syncSpecDrafts([createSpecDraft("颜色", ["红色"])]),
        )
        expect(result.current.form.state.values.fields.skus).toEqual(before)
        act(() => bindings().syncSpecDrafts([]))
        expect(result.current.form.state.values.fields.skus).toEqual(before)
        act(() =>
            bindings().syncSpecDrafts([createSpecDraft("颜色", ["红色款"])]),
        )
        act(() => bindings().resetSpecDrafts())
        expect(result.current.form.state.values.specDrafts).toEqual([])
        expect(result.current.form.state.values.fields.skus).toEqual(before)
    })
    it("blocks save and final submission until specification drafts are applied", async () => {
        const { result, bindings } = setupSpecs()
        act(() =>
            bindings().syncSpecDrafts([createSpecDraft("颜色", ["红色"])]),
        )
        act(() => result.current.form.setFieldValue("changeReason", "规格调整"))
        act(() => result.current.requestSave(result.current.form.state.values))
        expect(result.current.saveOpen).toBe(false)
        expect(result.current.activeSection).toBe("sku")
        expect(result.current.formError).toContain("尚未应用")
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(mocks.revise).not.toHaveBeenCalled()
        expect(result.current.form.state.values.fields.skus[0].salePrice).toBe(
            "568.00",
        )
    })
    it("requires confirmation for removed SKUs and preserves the table on cancellation", () => {
        const { result, bindings, confirm } = setupSpecs()
        const before = structuredClone(result.current.form.state.values.fields)
        act(() =>
            bindings().syncSpecDrafts([createSpecDraft("颜色", ["红色"])]),
        )
        act(() => {
            expect(bindings().applySpecDrafts()).toBeNull()
        })
        expect(confirm).toHaveBeenCalledWith(
            expect.objectContaining({
                details: ["SKU-01 · 默认规格"],
                confirmLabel: "应用规格",
            }),
        )
        expect(result.current.form.state.values.fields).toEqual(before)
        act(() => {
            confirm.mock.calls[0][0].onConfirm()
        })
        expect(result.current.form.state.values.fields.specs).toEqual([
            { name: "颜色", values: ["红色"] },
        ])
        expect(mocks.revise).not.toHaveBeenCalled()
    })
    it("fills empty prices directly, restores them on undo and only warns about submitted price fields", () => {
        const { result, bindings, confirm, offerUndo } = setupSpecs()
        act(() =>
            bindings().updateSku(0, { salePrice: "", marketPrice: "900.00" }),
        )
        act(() => result.current.form.setFieldValue("batchSalePrice", "600.00"))
        act(() => bindings().applyBatchReferencePrices())
        expect(confirm).not.toHaveBeenCalled()
        expect(result.current.form.state.values.fields.skus[0]).toMatchObject({
            salePrice: "600.00",
            marketPrice: "900.00",
        })
        act(() => offerUndo.mock.calls[0][0]())
        expect(result.current.form.state.values.fields.skus[0]).toMatchObject({
            salePrice: "",
            marketPrice: "900.00",
        })
        act(() => bindings().updateSku(0, { salePrice: "500.00" }))
        act(() => bindings().applyBatchReferencePrices())
        expect(confirm.mock.calls[0][0].description).toContain("销售价")
        expect(confirm.mock.calls[0][0].description).not.toContain("市场价")
        expect(result.current.form.state.values.fields.skus[0].salePrice).toBe(
            "500.00",
        )
    })
    it("rejects incomplete and duplicate specifications before modifying SKUs", () => {
        const { result, bindings, confirm } = setupSpecs()
        const before = structuredClone(result.current.form.state.values.fields)
        act(() => bindings().syncSpecDrafts([createSpecDraft("颜色", [""])]))
        expect(bindings().applySpecDrafts()).toContain("补全")
        act(() =>
            bindings().syncSpecDrafts([
                createSpecDraft("颜色", ["红色", " 红色 "]),
            ]),
        )
        expect(bindings().applySpecDrafts()).toContain("重复")
        expect(result.current.form.state.values.fields).toEqual(before)
        expect(confirm).not.toHaveBeenCalled()
    })
    it("opens confirmation without writing or changing effective dates, and keeps edits when closed", () => {
        const { result } = renderHook(() => useProductEditor("product-1"))
        act(() => result.current.form.setFieldValue("name", "修改后的礼盒"))
        act(() => result.current.requestSave(result.current.form.state.values))
        expect(result.current.saveOpen).toBe(true)
        expect(mocks.revise).not.toHaveBeenCalled()
        expect(result.current.form.state.values.effectiveFrom).toBe(
            "2026-08-30",
        )
        act(() => result.current.setSaveOpen(false))
        expect(result.current.form.state.values.name).toBe("修改后的礼盒")
    })
    it("requires the change reason at final confirmation", async () => {
        const { result } = renderHook(() => useProductEditor("product-1"))
        act(() => result.current.requestSave(result.current.form.state.values))
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.saveOpen).toBe(true)
        expect(result.current.saveAttempted).toBe(true)
        expect(result.current.formError).toContain("变更原因")
        expect(mocks.revise).not.toHaveBeenCalled()
    })
    it("preserves edits on conflict and closes only after success with the same revision contract", async () => {
        const { result } = renderHook(() => useProductEditor("product-1"))
        act(() =>
            result.current.form.setFieldValue("changeReason", "调整商品资料"),
        )
        act(() => result.current.requestSave(result.current.form.state.values))
        mocks.revise.mockResolvedValueOnce({
            outcome: "conflict",
            message: "资料已更新",
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.saveOpen).toBe(true)
        expect(result.current.result?.outcome).toBe("conflict")
        expect(result.current.form.state.values.changeReason).toBe(
            "调整商品资料",
        )
        mocks.revise.mockResolvedValueOnce({
            outcome: "succeeded",
            stableNo: "P-1",
            revisionNo: 2,
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.saveOpen).toBe(false)
        expect(mocks.revise).toHaveBeenLastCalledWith(
            expect.objectContaining({
                baseRevisionId: "revision-1",
                expectedLockVersion: 7,
                effectiveFrom: "2026-08-30",
                changeReason: "调整商品资料",
            }),
        )
        expect(mocks.refetch).toHaveBeenCalled()
    })
    it("blocks read-only accounts", async () => {
        mocks.permissions = []
        const { result } = renderHook(() => useProductEditor("product-1"))
        act(() => result.current.requestSave(result.current.form.state.values))
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.saveOpen).toBe(false)
        expect(mocks.revise).not.toHaveBeenCalled()
    })
    it("routes missing SKU images back to SKU editing before confirmation", () => {
        const { result } = renderHook(() => useProductEditor("product-1"))
        act(() =>
            result.current.form.setFieldValue("fields.skus[0].mainImage", ""),
        )
        act(() => result.current.requestSave(result.current.form.state.values))
        expect(result.current.saveOpen).toBe(false)
        expect(result.current.activeSection).toBe("sku")
        expect(result.current.formError).toContain("主图")
        expect(mocks.revise).not.toHaveBeenCalled()
    })
    it("restores the legacy effective link into a dismissible save dialog", () => {
        window.history.replaceState(
            null,
            "",
            "/master-data/products/product-1#product-section-effective",
        )
        const { result } = renderHook(() => useProductEditor("product-1"))
        expect(result.current.saveOpen).toBe(true)
        expect(result.current.activeSection).toBe("basic")
        act(() => result.current.setSaveOpen(false))
        expect(result.current.saveOpen).toBe(false)
    })
})
