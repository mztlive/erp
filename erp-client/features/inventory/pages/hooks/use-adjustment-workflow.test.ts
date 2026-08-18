import { beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import { useAdjustmentWorkflow } from "./use-adjustment-workflow"
import type { StockBalanceRow } from "@/features/inventory/types"

const mocks = vi.hoisted(() => ({
    createDraftMutateAsync: vi.fn(),
    submitMutateAsync: vi.fn(),
    resolveMutateAsync: vi.fn(),
}))

vi.mock("@/features/inventory/hooks/queries", () => ({
    useCreateAdjustmentDraftMutation: () => ({
        mutateAsync: mocks.createDraftMutateAsync,
        isPending: false,
    }),
    useSubmitAdjustmentMutation: () => ({
        mutateAsync: mocks.submitMutateAsync,
        isPending: false,
    }),
    useResolveAdjustmentUnknownMutation: () => ({
        mutateAsync: mocks.resolveMutateAsync,
        isPending: false,
    }),
}))

const rowFixture: StockBalanceRow = {
    balanceId: "bal_1",
    warehouseId: "w1",
    warehouseCode: "WH-1",
    warehouseName: "华东仓",
    skuId: "sku_1",
    skuCode: "SKU-1",
    skuName: "测试品",
    specSummary: "500ml",
    baseUnit: "件",
    onHandQuantity: "10",
    reservedQuantity: "2",
    availableQuantity: "8",
    lockVersion: 5,
    lastMovementId: "",
    lastMovementAt: "",
    lastMovementTypeLabel: "",
    availability: "positive",
    statusLabel: "有可用",
    statusTone: "success",
    hasActiveReservation: false,
    stockKind: "OWN_PHYSICAL",
    allowedActions: ["CREATE_ADJUSTMENT", "VIEW_SOURCE"],
    actionBlockers: [],
}

const draftFixture = {
    stockAdjustmentId: "adj_1",
    adjustmentNo: "TZ-1",
    balanceId: "bal_1",
    warehouseId: "w1",
    warehouseName: "华东仓",
    skuId: "sku_1",
    skuCode: "SKU-1",
    skuName: "测试品",
    baseUnit: "件",
    reasonType: "COUNT_LOSS" as const,
    quantity: "",
    note: "",
    occurredAt: "2026-08-14T10:00:00Z",
    balanceLockVersion: 3,
    editVersion: 2,
    segregationNote: "岗位分离说明",
}

function setup(overrides: { isPhoneNarrow?: boolean } = {}) {
    const onFocusRestore = vi.fn()
    const onPreviewClose = vi.fn()
    const rendered = renderHookWithProviders(() =>
        useAdjustmentWorkflow({
            isPhoneNarrow: overrides.isPhoneNarrow ?? false,
            onFocusRestore,
            onPreviewClose,
        }),
    )
    return {
        result: rendered.result,
        rerender: rendered.rerender,
        unmount: rendered.unmount,
        onFocusRestore,
        onPreviewClose,
    }
}

async function startAdjustmentSuccess(
    result: { current: ReturnType<typeof useAdjustmentWorkflow> },
    row: StockBalanceRow = rowFixture,
) {
    mocks.createDraftMutateAsync.mockResolvedValue(draftFixture)
    await act(async () => {
        await result.current.startAdjustment(row)
    })
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useAdjustmentWorkflow", () => {
    it("starts closed with no result, no error and no draft", () => {
        const { result } = setup()
        expect(result.current.adjustDraftId).toBeNull()
        expect(result.current.adjustMeta).toBeNull()
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.lastResult).toBeNull()
        expect(result.current.actionError).toBeNull()
        expect(result.current.pendingPayload).toBeNull()
        expect(result.current.isSubmitting).toBe(false)
    })

    it("blocks adjustment on narrow screens without calling the API", async () => {
        const { result, onFocusRestore, onPreviewClose } = setup({
            isPhoneNarrow: true,
        })
        await act(async () => {
            await result.current.startAdjustment(rowFixture)
        })
        expect(result.current.actionError).toBe(
            "窄屏（移动端）仅支持只读查询；库存调整请在桌面完成。",
        )
        expect(mocks.createDraftMutateAsync).not.toHaveBeenCalled()
        expect(onFocusRestore).not.toHaveBeenCalled()
        expect(onPreviewClose).not.toHaveBeenCalled()
    })

    it("uses the blocker message when CREATE_ADJUSTMENT is not allowed", async () => {
        const { result } = setup()
        const blockedRow: StockBalanceRow = {
            ...rowFixture,
            allowedActions: ["VIEW_SOURCE"],
            actionBlockers: [
                {
                    action: "CREATE_ADJUSTMENT",
                    code: "BALANCE_LOCKED",
                    message: "当前不允许发起库存调整",
                },
            ],
        }
        await act(async () => {
            await result.current.startAdjustment(blockedRow)
        })
        expect(result.current.actionError).toBe("当前不允许发起库存调整")
        expect(mocks.createDraftMutateAsync).not.toHaveBeenCalled()
    })

    it("creates a draft, records meta, seeds the form and closes the preview", async () => {
        const { result, onFocusRestore, onPreviewClose } = setup()
        await startAdjustmentSuccess(result)

        expect(onFocusRestore).toHaveBeenCalledWith("bal_1")
        expect(onPreviewClose).toHaveBeenCalledTimes(1)
        expect(mocks.createDraftMutateAsync).toHaveBeenCalledWith({
            balanceId: "bal_1",
        })
        expect(result.current.adjustDraftId).toBe("adj_1")
        expect(result.current.adjustMeta).toEqual({
            stockAdjustmentId: "adj_1",
            warehouseName: "华东仓",
            skuCode: "SKU-1",
            skuName: "测试品",
            baseUnit: "件",
            onHand: "10",
            available: "8",
            adjustmentNo: "TZ-1",
            editVersion: 2,
            segregationNote: "岗位分离说明",
            approval: undefined,
        })
        expect(result.current.form.state.values).toEqual({
            reasonType: "COUNT_LOSS",
            quantity: "",
            note: "",
            occurredAt: "2026-08-14T10:00",
        })
        expect(result.current.actionError).toBeNull()
        expect(result.current.lastResult).toBeNull()
    })

    it("shows the draft error message when creating the draft fails", async () => {
        const { result } = setup()
        mocks.createDraftMutateAsync.mockRejectedValue(
            new Error("创建调整草稿失败：余额已变化"),
        )
        await act(async () => {
            await result.current.startAdjustment(rowFixture)
        })
        expect(result.current.actionError).toBe("创建调整草稿失败：余额已变化")
        expect(result.current.adjustDraftId).toBeNull()
    })

    it("submits the form values with lock versions and a stable idempotency key", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("reasonType", "DAMAGE")
            result.current.form.setFieldValue("quantity", " 3.5 ")
            result.current.form.setFieldValue("note", " 破损报废 ")
            result.current.form.setFieldValue("occurredAt", "2026-08-13T09:30")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "SUBMITTED_FOR_APPROVAL",
                stockAdjustmentId: "adj_1",
                adjustmentNo: "TZ-2",
                nextResponsible: "李四",
                currentNodeLabel: "仓储审核",
                reference: "TZ-2",
                submittedAt: "2026-08-14T11:00:00Z",
                balanceLockVersion: 3,
            },
        })

        await act(async () => {
            await result.current.doSubmit()
        })

        expect(mocks.submitMutateAsync).toHaveBeenCalledWith({
            stockAdjustmentId: "adj_1",
            expectedBalanceLockVersion: 3,
            seedBalanceLockVersion: 5,
            reasonType: "DAMAGE",
            reasonTypeLabel: "损坏",
            direction: "decrease",
            quantity: "3.5",
            note: "破损报废",
            occurredAt: "2026-08-13T09:30",
            idempotencyKey: expect.stringMatching(/^w10-adj-adj_1-\d+$/),
        })
        expect(result.current.lastResult).toEqual({
            status: "succeeded",
            title: "调整已提交审批",
            description:
                "单号 TZ-2。当前节点：仓储审核。当前审批人：李四。余额尚未变化，审批通过后由系统更新。",
            reference: "TZ-2",
        })
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.adjustDraftId).toBeNull()
        expect(result.current.adjustMeta).toBeNull()
    })

    it("keeps the draft open and records an unknown result when the outcome is unknown", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "1")
            result.current.form.setFieldValue("note", "盘点差异")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "unknown",
            message: "暂无法确认",
            idempotencyKey: "k-1",
        })

        await act(async () => {
            await result.current.doSubmit()
        })

        expect(result.current.lastResult).toEqual({
            status: "unknown",
            title: expect.any(String),
            description: "暂无法确认",
            reference: "k-1",
            pendingIdempotencyKey: "k-1",
        })
        expect(result.current.adjustDraftId).toBe("adj_1")
        expect(result.current.pendingPayload).toMatchObject({
            stockAdjustmentId: "adj_1",
            expectedBalanceLockVersion: 3,
            idempotencyKey: expect.stringMatching(/^w10-adj-adj_1-\d+$/),
        })
    })

    it("adopts the latest lock version on VERSION_CONFLICT and reports the error", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "2")
            result.current.form.setFieldValue("note", "盘点差异")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "failed",
            code: "VERSION_CONFLICT",
            message: "数据已变更，请刷新后重试",
            latestLockVersion: 9,
        })

        await act(async () => {
            await result.current.doSubmit()
        })

        expect(result.current.actionError).toBe("数据已变更，请刷新后重试")
        expect(result.current.adjustDraftId).toBe("adj_1")
        expect(result.current.confirmOpen).toBe(false)

        // 下一次提交应带上最新核对版本
        mocks.submitMutateAsync.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "SUBMITTED_FOR_APPROVAL",
                stockAdjustmentId: "adj_1",
                adjustmentNo: "TZ-3",
                nextResponsible: "李四",
                currentNodeLabel: "仓储审核",
                reference: "TZ-3",
                submittedAt: "2026-08-14T11:00:00Z",
                balanceLockVersion: 9,
            },
        })
        await act(async () => {
            await result.current.doSubmit()
        })
        expect(mocks.submitMutateAsync).toHaveBeenLastCalledWith(
            expect.objectContaining({ expectedBalanceLockVersion: 9 }),
        )
        expect(result.current.lastResult?.status).toBe("succeeded")
    })

    it("reports generic failures without closing the draft", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "2")
            result.current.form.setFieldValue("note", "盘点差异")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "failed",
            code: "INVALID",
            message: "数量格式不正确",
        })

        await act(async () => {
            await result.current.doSubmit()
        })

        expect(result.current.actionError).toBe("数量格式不正确")
        expect(result.current.adjustDraftId).toBe("adj_1")
    })

    it("resolveLastUnknown adopts the succeeded outcome and closes the workflow", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "1")
            result.current.form.setFieldValue("note", "盘点差异")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "unknown",
            message: "暂无法确认",
            idempotencyKey: "k-1",
        })
        await act(async () => {
            await result.current.doSubmit()
        })

        mocks.resolveMutateAsync.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "SUBMITTED_FOR_APPROVAL",
                stockAdjustmentId: "adj_1",
                adjustmentNo: "TZ-4",
                nextResponsible: "李四",
                currentNodeLabel: "仓储审核",
                reference: "TZ-4",
                submittedAt: "2026-08-14T11:00:00Z",
                balanceLockVersion: 3,
            },
        })
        await act(async () => {
            await result.current.resolveLastUnknown()
        })

        expect(mocks.resolveMutateAsync).toHaveBeenCalledWith({
            idempotencyKey: "k-1",
            stockAdjustmentId: "adj_1",
            expectedBalanceLockVersion: 3,
        })
        expect(result.current.lastResult).toEqual({
            status: "succeeded",
            title: "调整已提交审批",
            description: "单号 TZ-4。当前节点：仓储审核。当前审批人：李四。",
            reference: "TZ-4",
        })
        expect(result.current.adjustDraftId).toBeNull()
    })

    it("resolveLastUnknown reports failures as action errors", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "1")
            result.current.form.setFieldValue("note", "盘点差异")
        })
        mocks.submitMutateAsync.mockResolvedValue({
            status: "unknown",
            message: "暂无法确认",
            idempotencyKey: "k-2",
        })
        await act(async () => {
            await result.current.doSubmit()
        })

        mocks.resolveMutateAsync.mockResolvedValue({
            status: "failed",
            code: "NO_PENDING",
            message: "未找到该任务号对应的处理中请求",
        })
        await act(async () => {
            await result.current.resolveLastUnknown()
        })
        expect(result.current.actionError).toBe(
            "未找到该任务号对应的处理中请求",
        )
    })

    it("opens the confirm dialog when the form passes validation", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.form.setFieldValue("quantity", "1")
            result.current.form.setFieldValue("note", "盘点差异说明")
        })

        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.confirmOpen).toBe(true)
    })

    it("does not open the confirm dialog when the form is invalid", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)

        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(result.current.confirmOpen).toBe(false)
        expect(mocks.submitMutateAsync).not.toHaveBeenCalled()
    })

    it("closeAdjustment resets the whole workflow", async () => {
        const { result } = setup()
        await startAdjustmentSuccess(result)
        act(() => {
            result.current.closeAdjustment()
        })
        expect(result.current.adjustDraftId).toBeNull()
        expect(result.current.adjustMeta).toBeNull()
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.pendingPayload).toBeNull()
    })
})
