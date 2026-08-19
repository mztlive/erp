import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    ensureSupplierPaymentDraft,
    fetchAllocationSession,
    resolveUnknownResult,
    saveAllocationDraft,
    submitInvoice,
    submitPayment,
} from "@/features/supplier-payables/api/requests"
import {
    useAllocationSession,
    type AllocationSessionParams,
} from "@/features/supplier-payables/hooks/use-allocation-session"
import type {
    AllocationSessionView,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { renderHookWithProviders } from "@/features/test-utils"

vi.mock("@/features/supplier-payables/api/requests", () => ({
    fetchAllocationSession: vi.fn(),
    fetchPayableDetail: vi.fn(),
    fetchSupplierAccounts: vi.fn(),
    resolveUnknownResult: vi.fn(),
    reverseInvoice: vi.fn(),
    reversePayment: vi.fn(),
    saveAllocationDraft: vi.fn(),
    submitInvoice: vi.fn(),
    submitPayment: vi.fn(),
    ensureSupplierPaymentDraft: vi.fn(),
}))

const SUCCESS_RESULT: FormalSubmitResult = {
    status: "succeeded",
    title: "付款已提交审批",
    description: "已进入审批。全部节点通过后过账并核销。",
    reference: "FK-0001",
    documentNo: "FK-0001",
    allocatedTotal: "60.00",
    unallocatedAmount: "0.00",
    operationId: "op-1",
}

const UNKNOWN_RESULT: FormalSubmitResult = {
    status: "unknown",
    title: "处理结果待确认",
    description: "请勿重复提交",
    operationId: "op-1",
}

function makeSession(
    overrides: Partial<AllocationSessionView> = {},
): AllocationSessionView {
    return {
        draftSessionId: "alloc_sup_1",
        track: "payment",
        supplierId: "sup-1",
        supplierName: "上海示例供应商",
        pool: [
            {
                payableAccountId: "pa-1",
                primaryEntryId: "pe-1",
                entryLockVersion: 1,
                accountLockVersion: 1,
                sourceType: "PURCHASE_ORDER",
                sourceTypeLabel: "采购单",
                sourceDocumentNo: "PO-1001",
                sourceDocumentId: "po-1",
                openTotal: "100.00",
                openInvoiceableTotal: "80.00",
                dueDate: "2026-08-20",
                dueStateLabel: "未到期",
                statusLabel: "未结",
            },
            {
                payableAccountId: "pa-2",
                primaryEntryId: "pe-2",
                entryLockVersion: 1,
                accountLockVersion: 1,
                sourceType: "SUPPLIER_SETTLEMENT",
                sourceTypeLabel: "供应商结算单",
                sourceDocumentNo: "JS-2002",
                sourceDocumentId: "st-2",
                openTotal: "50.00",
                openInvoiceableTotal: "40.00",
                dueDate: "2026-08-25",
                dueStateLabel: "未到期",
                statusLabel: "未结",
            },
        ],
        payablePriorityPolicy: {
            state: "AVAILABLE",
            mixedAutoAllocationAllowed: true,
        },
        preselectedPayableAccountIds: [],
        dataWatermark: "wm-sess-2",
        queriedAt: "2026-08-14T00:00:00.000Z",
        ...overrides,
    }
}

function renderSession(
    params: Partial<AllocationSessionParams> = {},
    options: { onCompleted?: (r: FormalSubmitResult) => void } = {},
) {
    return renderHookWithProviders(
        () =>
            useAllocationSession(
                { track: "payment", supplierId: "sup-1", ...params },
                options,
            ),
        { queryClient: undefined },
    )
}

async function loadSession(result: ReturnType<typeof renderSession>["result"]) {
    await waitFor(() => {
        expect(result.current.session).toBeDefined()
    })
}

beforeEach(() => {
    vi.mocked(fetchAllocationSession).mockReset()
    vi.mocked(saveAllocationDraft).mockReset()
    vi.mocked(submitPayment).mockReset()
    vi.mocked(submitInvoice).mockReset()
    vi.mocked(resolveUnknownResult).mockReset()
    vi.mocked(ensureSupplierPaymentDraft).mockReset()
    vi.mocked(fetchAllocationSession).mockResolvedValue(makeSession())
    vi.mocked(ensureSupplierPaymentDraft).mockResolvedValue({
        status: "succeeded",
        payment: {
            id: "pay-1",
            payment_no: "FK-0001",
            status: "draft",
            supplier_id: "sup-1",
            paid_at: 1,
            amount: "60.00",
            version: 1,
            created_at: 1,
            allocated_total: "0.00",
            unallocated_amount: "60.00",
            allocations: [],
        },
    })
})

describe("useAllocationSession", () => {
    it("loads the session and reports initial validation issues", async () => {
        const { result } = renderSession()
        expect(result.current.sessionQuery.isPending).toBe(true)
        await loadSession(result)

        expect(vi.mocked(fetchAllocationSession)).toHaveBeenCalledWith({
            track: "payment",
            supplierId: "sup-1",
            draftSessionId: undefined,
            purchaseOrderId: undefined,
            returnTo: undefined,
            fromWorkspace: undefined,
            existingPaymentId: undefined,
            existingInvoiceId: undefined,
            preselectPayableAccountId: undefined,
        })
        expect(result.current.selected.size).toBe(0)
        const ids = result.current.issues.map((i) => i.id)
        expect(ids).toContain("no-target")
        expect(ids).toContain("amount")
        expect(result.current.canSubmit).toBe(false)
    })

    it("does not fetch when supplierId is empty (query disabled)", () => {
        renderSession({ supplierId: "" })
        expect(vi.mocked(fetchAllocationSession)).not.toHaveBeenCalled()
    })

    it("prefills selection, per-target amounts and the payment amount from preselected ids", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({
                preselectedPayableAccountIds: ["pa-1", "pa-2"],
            }),
        )
        const { result } = renderSession()
        await loadSession(result)

        expect(result.current.selected).toEqual(new Set(["pa-1", "pa-2"]))
        expect(result.current.amounts["pa-1"]).toBe("100.00")
        expect(result.current.amounts["pa-2"]).toBe("50.00")
        expect(result.current.paymentForm.getFieldValue("amount")).toBe(
            "150.00",
        )
        expect(result.current.allocatedHint).toBe("150.00")
    })

    it("prefills the invoice gross amount from open invoiceable totals", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({
                track: "purchase_invoice",
                preselectedPayableAccountIds: ["pa-1"],
            }),
        )
        const { result } = renderSession({ track: "purchase_invoice" })
        await loadSession(result)

        expect(result.current.amounts["pa-1"]).toBe("80.00")
        expect(result.current.invoiceForm.getFieldValue("grossAmount")).toBe(
            "80.00",
        )
        expect(result.current.factAmount).toBe("80.00")
    })

    it("syncs existing unallocated into the fact form", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({
                existingPaymentId: "pmt-1",
                existingAmount: "100.00",
                existingUnallocated: "40.00",
                existingDocumentNo: "FK-100",
            }),
        )
        const { result } = renderSession()
        await loadSession(result)

        expect(result.current.paymentForm.getFieldValue("amount")).toBe(
            "40.00",
        )
        expect(result.current.factAmount).toBe("40.00")
    })

    it("toggles a single target on and off, prefilling its open total on select", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
        })
        expect(result.current.selected.has("pa-1")).toBe(true)
        expect(result.current.amounts["pa-1"]).toBe("100.00")

        act(() => {
            result.current.toggleItem("pa-1", false, "100.00")
        })
        expect(result.current.selected.has("pa-1")).toBe(false)
        // 取消勾选不清空已填金额
        expect(result.current.amounts["pa-1"]).toBe("100.00")
    })

    it("toggleSelectAll selects everything with filled amounts, and clears on second call", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleSelectAll()
        })
        expect(result.current.selected.size).toBe(2)
        expect(result.current.amounts["pa-1"]).toBe("100.00")
        expect(result.current.amounts["pa-2"]).toBe("50.00")

        act(() => {
            result.current.toggleSelectAll()
        })
        expect(result.current.selected.size).toBe(0)
    })

    it("fillAllSelected refills only selected targets", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "10.00")
        })
        act(() => {
            result.current.fillAllSelected()
        })
        expect(result.current.amounts["pa-1"]).toBe("100.00")
        expect(result.current.amounts["pa-2"]).toBeUndefined()
    })

    it("reports per-target over-allocation and non-positive amount issues", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "150.00")
        })
        expect(
            result.current.issues.some((i) => i.id === "over-pa-1"),
        ).toBe(true)
        expect(
            result.current.issues.find((i) => i.id === "over-pa-1")?.message,
        ).toBe("拟分配超过开放余额 100.00")
        expect(result.current.canSubmit).toBe(false)

        act(() => {
            result.current.setAmountFor("pa-1", "0")
        })
        expect(
            result.current.issues.some((i) => i.id === "zero-pa-1"),
        ).toBe(true)
    })

    it("becomes submittable when targets and fact amount are consistent", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "60.00")
            result.current.paymentForm.setFieldValue("amount", "60.00")
        })
        expect(result.current.issues).toEqual([])
        expect(result.current.canSubmit).toBe(true)
        expect(result.current.unallocatedHint).toBe("0.00")
    })

    it("doSubmit submits the payment with mapped targets and reports success", async () => {
        vi.mocked(submitPayment).mockResolvedValue(SUCCESS_RESULT)
        const onCompleted = vi.fn()
        const { result } = renderSession({}, { onCompleted })
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "60.00")
            result.current.paymentForm.setFieldValue("amount", "60.00")
        })
        await act(async () => {
            await result.current.doSubmit()
        })

        expect(submitPayment).toHaveBeenCalledTimes(1)
        const payload = vi.mocked(submitPayment).mock.calls[0][0]
        expect(payload).toMatchObject({
            draftSessionId: "alloc_sup_1",
            supplierId: "sup-1",
            amount: "60.00",
            bankReference: "",
            note: "",
            explicitSelection: true,
        })
        expect(payload.existingPaymentId).toBeUndefined()
        expect(payload.idempotencyKey).toMatch(
            /^w12_payment_alloc_sup_1_\d+$/,
        )
        expect(payload.paidAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/)
        expect(payload.targets).toEqual([
            {
                payableAccountId: "pa-1",
                payableEntryId: "pe-1",
                amount: "60.00",
                entryLockVersion: 1,
                accountLockVersion: 1,
            },
        ])
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "付款已提交审批",
            reference: "FK-0001",
            operationId: "op-1",
        })
        expect(result.current.result?.description).toContain("已进入审批")
        expect(result.current.confirmOpen).toBe(false)
        expect(onCompleted).toHaveBeenCalledWith(result.current.result)
    })

    it("doSubmit for an existing payment uses existingAmount and keeps unallocated fact", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({
                existingPaymentId: "pmt-1",
                existingAmount: "100.00",
                existingUnallocated: "40.00",
            }),
        )
        vi.mocked(submitPayment).mockResolvedValue(SUCCESS_RESULT)
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "20.00")
        })
        await act(async () => {
            await result.current.doSubmit()
        })

        const payload = vi.mocked(submitPayment).mock.calls[0][0]
        expect(payload.amount).toBe("100.00")
        expect(payload.existingPaymentId).toBe("pmt-1")
        expect(payload.targets[0].amount).toBe("20.00")
    })

    it("doSubmit records failed results without calling onCompleted", async () => {
        vi.mocked(submitPayment).mockResolvedValue({
            status: "failed",
            title: "付款失败",
            description: "boom",
            errorCode: "HTTP_ERROR",
        })
        const onCompleted = vi.fn()
        const { result } = renderSession(
            { returnTo: "/supplier-accounts" },
            { onCompleted },
        )
        await loadSession(result)

        await act(async () => {
            await result.current.doSubmit()
        })

        expect(result.current.result?.status).toBe("failed")
        expect(result.current.result?.returnTo).toBe("/supplier-accounts")
        expect(onCompleted).not.toHaveBeenCalled()
    })

    it("doSubmit for the invoice track calls submitInvoice with BLUE invoice data", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({ track: "purchase_invoice" }),
        )
        vi.mocked(submitInvoice).mockResolvedValue({
            ...SUCCESS_RESULT,
            title: "进项发票已登记",
        })
        const { result } = renderSession({ track: "purchase_invoice" })
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "80.00")
            result.current.setAmountFor("pa-1", "80.00")
            result.current.invoiceForm.setFieldValue("invoiceCode", "0110")
            result.current.invoiceForm.setFieldValue("invoiceNo", "INV-1")
            result.current.invoiceForm.setFieldValue("grossAmount", "80.00")
            result.current.invoiceForm.setFieldValue("netAmount", "65.00")
            result.current.invoiceForm.setFieldValue("taxAmount", "15.00")
        })
        await act(async () => {
            await result.current.doSubmit()
        })

        expect(submitInvoice).toHaveBeenCalledTimes(1)
        const payload = vi.mocked(submitInvoice).mock.calls[0][0]
        expect(payload).toMatchObject({
            draftSessionId: "alloc_sup_1",
            supplierId: "sup-1",
            invoiceCode: "0110",
            invoiceNo: "INV-1",
            grossAmount: "80.00",
            netAmount: "65.00",
            taxAmount: "15.00",
            invoiceKind: "BLUE",
            explicitSelection: true,
        })
        expect(payload.existingInvoiceId).toBeUndefined()
        expect(payload.idempotencyKey).toMatch(
            /^w12_purchase_invoice_alloc_sup_1_\d+$/,
        )
        expect(payload.targets).toEqual([
            {
                payableAccountId: "pa-1",
                payableEntryId: "pe-1",
                amount: "80.00",
                entryLockVersion: 1,
                accountLockVersion: 1,
            },
        ])
        expect(result.current.result?.status).toBe("succeeded")
    })

    it("handleSaveDraft persists a snapshot of selections and form values", async () => {
        vi.mocked(saveAllocationDraft).mockResolvedValue({
            savedAt: "2026-08-14T10:00:00.000Z",
        })
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "60.00")
        })
        await act(async () => {
            await result.current.handleSaveDraft()
        })

        expect(saveAllocationDraft).toHaveBeenCalledTimes(1)
        const [snapshotInput] = vi.mocked(saveAllocationDraft).mock.calls[0]
        expect(snapshotInput).toEqual({
            draftSessionId: "alloc_sup_1",
            track: "payment",
            supplierId: "sup-1",
            formSnapshot: {
                amounts: { "pa-1": "60.00" },
                selected: ["pa-1"],
                payment: {
                    paidAt: expect.stringMatching(
                        /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/,
                    ),
                    amount: "0.00",
                    bankReference: "",
                    note: "",
                },
                invoice: {
                    invoiceCode: "",
                    invoiceNo: "",
                    invoiceDate: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
                    grossAmount: "",
                    netAmount: "",
                    taxAmount: "",
                },
            },
        })
        expect(result.current.draftHint).toMatch(/^草稿已保存 /)
    })

    it("requestSubmit prepares the payment draft then opens confirm", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({
                existingPaymentId: "pmt-1",
                existingAmount: "100.00",
                existingUnallocated: "40.00",
            }),
        )
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.paymentForm.setFieldValue("bankReference", "BANK-1")
        })
        await act(async () => {
            result.current.requestSubmit()
        })
        await waitFor(() => {
            expect(result.current.confirmOpen).toBe(true)
        })
        expect(ensureSupplierPaymentDraft).toHaveBeenCalledTimes(1)
        expect(submitPayment).not.toHaveBeenCalled()
    })

    it("requestSubmit triggers form validation and opens the confirm dialog", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "60.00")
            result.current.paymentForm.setFieldValue("amount", "60.00")
            result.current.paymentForm.setFieldValue(
                "bankReference",
                "BANK-1",
            )
        })
        await act(async () => {
            result.current.requestSubmit()
        })
        await waitFor(() => {
            expect(result.current.confirmOpen).toBe(true)
        })
    })

    it("requestSubmit does not open the dialog while the form is invalid", async () => {
        const { result } = renderSession()
        await loadSession(result)

        act(() => {
            result.current.toggleItem("pa-1", true, "100.00")
            result.current.setAmountFor("pa-1", "60.00")
        })
        await act(async () => {
            result.current.requestSubmit()
            // 让表单校验的微任务与宏任务链走完，确认不会触发 onSubmit
            await new Promise((resolve) => setTimeout(resolve, 0))
        })
        expect(result.current.confirmOpen).toBe(false)
        expect(submitPayment).not.toHaveBeenCalled()
    })

    it("handleResolveUnknown replaces the result and notifies on success", async () => {
        vi.mocked(submitPayment).mockResolvedValue(UNKNOWN_RESULT)
        vi.mocked(resolveUnknownResult).mockResolvedValue(SUCCESS_RESULT)
        const onCompleted = vi.fn()
        const { result } = renderSession({}, { onCompleted })
        await loadSession(result)

        await act(async () => {
            await result.current.doSubmit()
        })
        expect(result.current.result?.status).toBe("unknown")

        let resolved = false
        await act(async () => {
            resolved = await result.current.handleResolveUnknown()
        })
        expect(resolved).toBe(true)
        expect(
            vi.mocked(resolveUnknownResult).mock.calls[0][0],
        ).toMatch(/^w12_payment_alloc_sup_1_\d+$/)
        expect(result.current.result?.status).toBe("succeeded")
        expect(onCompleted).toHaveBeenCalledWith(SUCCESS_RESULT)
    })

    it("handleResolveUnknown keeps the unknown result when nothing is found", async () => {
        vi.mocked(submitPayment).mockResolvedValue(UNKNOWN_RESULT)
        vi.mocked(resolveUnknownResult).mockResolvedValue(null)
        const { result } = renderSession()
        await loadSession(result)

        await act(async () => {
            await result.current.doSubmit()
        })

        let resolved = true
        await act(async () => {
            resolved = await result.current.handleResolveUnknown()
        })
        expect(resolved).toBe(false)
        expect(result.current.result?.status).toBe("unknown")
    })

    it("handles an empty pool without crashing", async () => {
        vi.mocked(fetchAllocationSession).mockResolvedValue(
            makeSession({ pool: [] }),
        )
        const { result } = renderSession()
        await loadSession(result)

        expect(result.current.issues.map((i) => i.id)).toContain("no-target")
        act(() => {
            result.current.toggleSelectAll()
        })
        expect(result.current.selected.size).toBe(0)
    })
})
