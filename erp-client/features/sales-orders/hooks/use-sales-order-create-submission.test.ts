import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook, act, waitFor } from "@testing-library/react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import * as salesOrdersApi from "@/features/sales-orders/api/sales-orders"
import * as salesOrdersQueries from "@/features/sales-orders/hooks/queries"
import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import { useSalesOrderCreateSubmission } from "./use-sales-order-create-submission"

const routerMock = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => routerMock,
    useSearchParams: () => new URLSearchParams(),
    usePathname: () => "/test",
    useParams: () => ({}),
}))

vi.mock("@/features/sales-orders/hooks/queries", () => ({
    useCreateSalesOrderMutation: vi.fn(),
    useSaveSalesOrderDraftMutation: vi.fn(),
    useSubmitSalesOrderMutation: vi.fn(),
    useResolveProcurementRejectionMutation: vi.fn(),
}))

vi.mock("@/features/sales-orders/api/sales-orders", () => ({
    prepareProcurementRejectionResolution: vi.fn(),
}))

const mockedQueries = vi.mocked(salesOrdersQueries)
const mockedApi = vi.mocked(salesOrdersApi)
const mockedRouter = vi.mocked(routerMock)

const createMutation = {
    mutateAsync: vi.fn(),
    isError: false,
    error: null,
    isPending: false,
}
const saveDraftMutation = { mutateAsync: vi.fn(), isPending: false }
const submitMutation = { mutateAsync: vi.fn(), isPending: false }
const resubmitMutation = { mutateAsync: vi.fn(), isPending: false }

const makeValues = (
    overrides: Partial<CreateSalesOrderFormValues> = {},
): CreateSalesOrderFormValues => ({
    contractId: "ct-1",
    requestedContractRevisionId: "r-1",
    contractRevisionLabel: "CT-1@v1",
    customerId: "cu-1",
    customerName: "客户甲",
    settlementPartyId: "sp-1",
    settlementEntity: "结算主体甲",
    nature: "physical_service",
    fulfillmentMode: "公司仓发",
    ownerUserId: "u-1",
    ownerName: "张三",
    welfareScene: "annual",
    paymentTerms: "POSTPAY_NET30",
    fulfillmentDeadline: "2026-09-30",
    targetMallId: "",
    receivableDueDate: "",
    taxRatePercent: "13.00",
    remark: "",
    lineItems: [
        {
            rowKey: "l1",
            name: "货物",
            sku: "sku-1",
            skuRevisionId: "sr-1",
            quantity: "1",
            unit: "件",
            unitPriceGross: "100.00",
            fulfillmentMode: "公司仓发",
            dueDate: "2026-09-01",
            faceValue: "",
            giftRate: "",
            cardForm: "",
        },
    ],
    ...overrides,
})

const makeDraft = (
    overrides: Partial<SalesOrderDraftResumeData> = {},
): SalesOrderDraftResumeData => ({
    salesOrderId: "so-1",
    documentNumber: "SO-2026-001",
    version: 3,
    contractId: "ct-1",
    nature: "physical_service",
    welfareScene: "annual",
    paymentTerms: "POSTPAY_NET30",
    fulfillmentDeadline: "2026-09-30",
    targetMallId: "",
    receivableDueDate: "",
    taxRatePercent: "13.00",
    remark: "",
    lineItems: makeValues().lineItems,
    ...overrides,
})

/** 每次 acquire 生成唯一键，便于断言账本在失败/未知后的保留行为。 */
const makeLedger = () => {
    let seq = 0
    return new FormalCommandKeyLedger((prefix) => `${prefix}:k${++seq}`)
}

const formStub = () => ({ reset: vi.fn() })

const renderSubmission = (props: {
    initialDraft?: SalesOrderDraftResumeData | null
    purpose?: "create" | "draft" | "resubmit"
    commandLedger: FormalCommandKeyLedger
    onResult?: (result: unknown) => void
    onSubmitted?: (salesOrderId: string) => void
}) =>
    renderHook(
        ({ initialDraft, purpose, commandLedger, onResult, onSubmitted }) =>
            useSalesOrderCreateSubmission({
                initialDraft: initialDraft ?? null,
                purpose: purpose ?? "create",
                commandLedger,
                onResult,
                onSubmitted,
            }),
        {
            initialProps: {
                initialDraft: props.initialDraft ?? null,
                purpose: props.purpose ?? "create",
                commandLedger: props.commandLedger,
                onResult: props.onResult,
                onSubmitted: props.onSubmitted,
            },
        },
    )

beforeEach(() => {
    vi.clearAllMocks()
    mockedQueries.useCreateSalesOrderMutation.mockReturnValue(
        createMutation as never,
    )
    mockedQueries.useSaveSalesOrderDraftMutation.mockReturnValue(
        saveDraftMutation as never,
    )
    mockedQueries.useSubmitSalesOrderMutation.mockReturnValue(
        submitMutation as never,
    )
    mockedQueries.useResolveProcurementRejectionMutation.mockReturnValue(
        resubmitMutation as never,
    )
})

describe("useSalesOrderCreateSubmission · create flow", () => {
    it("creates with the SUBMIT intent, resets the form and reports the new id", async () => {
        createMutation.mutateAsync.mockResolvedValue({
            salesOrderId: "so-9",
            documentNumber: "SO-9",
        })
        const onSubmitted = vi.fn()
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            onSubmitted,
        })

        result.current.submitIntentRef.current = "SUBMIT"
        const form = formStub()
        await act(async () => {
            await result.current.handleSubmit(makeValues(), form)
        })

        expect(createMutation.mutateAsync).toHaveBeenCalledTimes(1)
        const payload = createMutation.mutateAsync.mock.calls[0][0]
        expect(payload).toMatchObject({
            contract: {
                contractId: "ct-1",
                requestedContractRevisionId: "r-1",
            },
            nature: "physical_service",
            paymentTerms: "货到 30 天",
            intent: "SUBMIT",
        })
        expect(payload.idempotencyKey).toBe("sales:create:k1")
        expect(payload.orderNo).toMatch(/^XS\d+$/)
        expect(form.reset).toHaveBeenCalledTimes(1)
        expect(onSubmitted).toHaveBeenCalledWith("so-9")
        expect(mockedRouter.push).not.toHaveBeenCalled()
    })

    it("navigates to the sales order when no onSubmitted handler is given", async () => {
        createMutation.mutateAsync.mockResolvedValue({
            salesOrderId: "so-9",
            documentNumber: "SO-9",
        })
        const { result } = renderSubmission({ commandLedger: makeLedger() })

        result.current.submitIntentRef.current = "SUBMIT"
        await act(async () => {
            await result.current.handleSubmit(makeValues(), formStub())
        })

        expect(mockedRouter.push).toHaveBeenCalledWith("/sales/orders/so-9")
    })

    it("saves as a draft without resetting the form", async () => {
        createMutation.mutateAsync.mockResolvedValue({
            salesOrderId: "so-1",
            documentNumber: "SO-1",
            workingCopyVersion: 2,
        })
        const { result } = renderSubmission({ commandLedger: makeLedger() })

        const form = formStub()
        await act(async () => {
            await result.current.handleSubmit(makeValues(), form)
        })

        const payload = createMutation.mutateAsync.mock.calls[0][0]
        expect(payload.intent).toBe("SAVE_DRAFT")
        expect(form.reset).not.toHaveBeenCalled()
        await waitFor(() =>
            expect(result.current.draftIdentity).toEqual({
                salesOrderId: "so-1",
                documentNumber: "SO-1",
                version: 2,
            }),
        )
        expect(result.current.draftSaved?.documentNumber).toBe("SO-1")
    })

    it("marks a determinate failure and clears the ledger entry", async () => {
        createMutation.mutateAsync.mockRejectedValue(new Error("合同已失效"))
        const ledger = makeLedger()
        const { result } = renderSubmission({ commandLedger: ledger })

        await act(async () => {
            await expect(
                result.current.handleSubmit(makeValues(), formStub()),
            ).rejects.toThrow("合同已失效")
        })

        await waitFor(() =>
            expect(result.current.formalFailure).toEqual({
                unknown: false,
                description: "合同已失效",
            }),
        )
        expect(ledger.peek("create")).toBeUndefined()
    })

    it("keeps the ledger entry and reuses the same command on unknown outcome", async () => {
        createMutation.mutateAsync.mockRejectedValueOnce({
            kind: "Network",
            message: "连接中断",
        })
        createMutation.mutateAsync.mockResolvedValueOnce({
            salesOrderId: "so-2",
            documentNumber: "SO-2",
        })
        const ledger = makeLedger()
        const onSubmitted = vi.fn()
        const { result } = renderSubmission({
            commandLedger: ledger,
            onSubmitted,
        })

        result.current.submitIntentRef.current = "SUBMIT"
        await act(async () => {
            await expect(
                result.current.handleSubmit(makeValues(), formStub()),
            ).rejects.toThrow()
        })

        await waitFor(() =>
            expect(result.current.formalFailure?.unknown).toBe(true),
        )
        expect(result.current.formalFailure?.description).toBe(
            "当前整单输入已保留，请使用本次操作重试；确认前不要再次创建。",
        )
        expect(ledger.peek("create")).toBeDefined()

        await act(async () => {
            await result.current.handleSubmit(makeValues(), formStub())
        })

        const calls = createMutation.mutateAsync.mock.calls
        expect(calls).toHaveLength(2)
        expect(calls[0][0].idempotencyKey).toBe(calls[1][0].idempotencyKey)
        expect(calls[1][0].orderNo).toBe(calls[0][0].orderNo)
        expect(onSubmitted).toHaveBeenCalledWith("so-2")
        await waitFor(() => expect(result.current.formalFailure).toBeNull())
    })
})

describe("useSalesOrderCreateSubmission · existing draft flow", () => {
    it("saves an existing draft with its version and contract", async () => {
        saveDraftMutation.mutateAsync.mockResolvedValue({ version: 4 })
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
        })

        const form = formStub()
        await act(async () => {
            await result.current.handleSubmit(makeValues(), form)
        })

        expect(saveDraftMutation.mutateAsync).toHaveBeenCalledWith({
            nature: "physical_service",
            ownerUserId: "u-1",
            ownerName: "张三",
            welfareScene: "annual",
            paymentTerms: "货到 30 天",
            fulfillmentDeadline: "2026-09-30",
            targetMallId: "",
            receivableDueDate: "",
            taxRatePercent: "13.00",
            remark: "",
            lineItems: makeValues().lineItems,
            salesOrderId: "so-1",
            version: 3,
            contract: {
                contractId: "ct-1",
                requestedContractRevisionId: "r-1",
            },
        })
        expect(form.reset).not.toHaveBeenCalled()
        expect(submitMutation.mutateAsync).not.toHaveBeenCalled()
        await waitFor(() =>
            expect(result.current.draftIdentity?.version).toBe(4),
        )
        expect(result.current.draftSaved?.documentNumber).toBe("SO-2026-001")
    })

    it("saves the draft once more then submits it with the fresh version", async () => {
        saveDraftMutation.mutateAsync.mockResolvedValue({ version: 4 })
        submitMutation.mutateAsync.mockResolvedValue({ salesOrderId: "so-1" })
        const onSubmitted = vi.fn()
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
            onSubmitted,
        })

        result.current.submitIntentRef.current = "SUBMIT"
        const form = formStub()
        await act(async () => {
            await result.current.handleSubmit(makeValues(), form)
        })

        expect(saveDraftMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(submitMutation.mutateAsync).toHaveBeenCalledWith({
            salesOrderId: "so-1",
            version: 4,
            idempotencyKey: "sales:so-1:submit:k1",
        })
        expect(form.reset).toHaveBeenCalledTimes(1)
        expect(onSubmitted).toHaveBeenCalledWith("so-1")
    })

    it("reuses the pending submit command without re-saving after unknown outcome", async () => {
        submitMutation.mutateAsync.mockRejectedValueOnce({
            kind: "Network",
            message: "连接中断",
        })
        submitMutation.mutateAsync.mockResolvedValueOnce({
            salesOrderId: "so-1",
        })
        const ledger = makeLedger()
        // 模拟上一次提交在保存后已生成命令身份，但结果未知。
        ledger.acquire("submit-existing", "sales:so-1:submit", {
            salesOrderId: "so-1",
            version: 4,
        })
        const onSubmitted = vi.fn()
        const { result } = renderSubmission({
            commandLedger: ledger,
            initialDraft: makeDraft(),
            onSubmitted,
        })

        result.current.submitIntentRef.current = "SUBMIT"
        await act(async () => {
            await expect(
                result.current.handleSubmit(makeValues(), formStub()),
            ).rejects.toThrow()
        })
        await waitFor(() =>
            expect(result.current.formalFailure?.unknown).toBe(true),
        )

        await act(async () => {
            await result.current.handleSubmit(makeValues(), formStub())
        })

        expect(saveDraftMutation.mutateAsync).not.toHaveBeenCalled()
        const calls = submitMutation.mutateAsync.mock.calls
        expect(calls).toHaveLength(2)
        expect(calls[0][0].idempotencyKey).toBe(calls[1][0].idempotencyKey)
        expect(onSubmitted).toHaveBeenCalledWith("so-1")
    })
})

describe("useSalesOrderCreateSubmission · resubmit flow", () => {
    it("opens the resubmit dialog instead of saving when a resolution is pending", async () => {
        const ledger = makeLedger()
        ledger.acquire("procurement-rejection-resolution", "sales:so-1:rr", {
            salesOrderId: "so-1",
            action: "RESUBMIT_CHANGED_TERMS",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sb-1",
            expectedSalesOrderLockVersion: 1,
            customerReconfirmationEvidenceIds: ["e-1"],
        } as never)
        const { result } = renderSubmission({
            commandLedger: ledger,
            initialDraft: makeDraft(),
            purpose: "resubmit",
        })

        await act(async () => {
            await result.current.handleSubmit(makeValues(), formStub())
        })

        expect(result.current.resubmitConfirmOpen).toBe(true)
        expect(saveDraftMutation.mutateAsync).not.toHaveBeenCalled()
        expect(resubmitMutation.mutateAsync).not.toHaveBeenCalled()
    })

    it("saves the edited content then opens the resubmit dialog on submit", async () => {
        saveDraftMutation.mutateAsync.mockResolvedValue({ version: 4 })
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
            purpose: "resubmit",
        })

        result.current.submitIntentRef.current = "SUBMIT"
        await act(async () => {
            await result.current.handleSubmit(makeValues(), formStub())
        })

        expect(saveDraftMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(submitMutation.mutateAsync).not.toHaveBeenCalled()
        expect(result.current.resubmitConfirmOpen).toBe(true)
    })

    it("rejects a resubmit confirm without evidence ids", async () => {
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
            purpose: "resubmit",
        })

        await expect(result.current.confirmResubmit()).rejects.toThrow(
            "请至少填写一项客户重新确认依据 ID",
        )
        expect(resubmitMutation.mutateAsync).not.toHaveBeenCalled()
    })

    it("resubmits with parsed evidence ids and reports the outcome", async () => {
        mockedApi.prepareProcurementRejectionResolution.mockResolvedValue({
            salesOrderId: "so-1",
            action: "RESUBMIT_CHANGED_TERMS",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sb-1",
            expectedSalesOrderLockVersion: 2,
            customerReconfirmationEvidenceIds: ["e-1"],
        })
        resubmitMutation.mutateAsync.mockResolvedValue({
            detail: "已生成新一版并再次报给采购",
            reference: "SO-2026-001",
        })
        const onResult = vi.fn()
        const onSubmitted = vi.fn()
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
            purpose: "resubmit",
            onResult,
            onSubmitted,
        })

        await act(async () => {
            result.current.setResubmitEvidence(" e-1, e-2，e-3；e-1 ")
        })
        await act(async () => {
            await result.current.confirmResubmit()
        })

        expect(
            mockedApi.prepareProcurementRejectionResolution,
        ).toHaveBeenCalledWith({
            salesOrderId: "so-1",
            action: "RESUBMIT_CHANGED_TERMS",
            customerReconfirmationEvidenceIds: ["e-1", "e-2", "e-3"],
        })
        expect(resubmitMutation.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                idempotencyKey: "sales:so-1:procurement-resubmit:k1",
            }),
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "已改完并再报给采购",
            description: "已生成新一版并再次报给采购",
            reference: "SO-2026-001",
            nextResponsible: "采购重新确认",
        })
        expect(onSubmitted).toHaveBeenCalledWith("so-1")
    })

    it("reports a determinate resubmit failure as blocked", async () => {
        mockedApi.prepareProcurementRejectionResolution.mockResolvedValue({
            salesOrderId: "so-1",
            action: "RESUBMIT_CHANGED_TERMS",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sb-1",
            expectedSalesOrderLockVersion: 2,
            customerReconfirmationEvidenceIds: ["e-1"],
        })
        resubmitMutation.mutateAsync.mockRejectedValue(
            new Error("商品与价格未变化"),
        )
        const onResult = vi.fn()
        const { result } = renderSubmission({
            commandLedger: makeLedger(),
            initialDraft: makeDraft(),
            purpose: "resubmit",
            onResult,
        })

        await act(async () => {
            result.current.setResubmitEvidence("e-1")
        })
        await act(async () => {
            await expect(result.current.confirmResubmit()).rejects.toThrow(
                "商品与价格未变化",
            )
        })

        expect(onResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "还不能再报给采购",
            description: "商品与价格未变化",
            reference: "SO-2026-001",
        })
    })

    it("keeps the resolution command and reuses it on unknown outcome", async () => {
        mockedApi.prepareProcurementRejectionResolution.mockResolvedValue({
            salesOrderId: "so-1",
            action: "RESUBMIT_CHANGED_TERMS",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sb-1",
            expectedSalesOrderLockVersion: 2,
            customerReconfirmationEvidenceIds: ["e-1"],
        })
        resubmitMutation.mutateAsync.mockRejectedValueOnce({
            kind: "Network",
            message: "连接中断",
        })
        resubmitMutation.mutateAsync.mockResolvedValueOnce({
            detail: "已再报",
            reference: "SO-2026-001",
        })
        const ledger = makeLedger()
        const onResult = vi.fn()
        const { result } = renderSubmission({
            commandLedger: ledger,
            initialDraft: makeDraft(),
            purpose: "resubmit",
            onResult,
        })

        await act(async () => {
            result.current.setResubmitEvidence("e-1")
        })
        await act(async () => {
            await expect(result.current.confirmResubmit()).rejects.toThrow()
        })
        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: "unknown" }),
        )
        expect(ledger.peek("procurement-rejection-resolution")).toBeDefined()

        await act(async () => {
            await result.current.confirmResubmit()
        })

        expect(
            mockedApi.prepareProcurementRejectionResolution,
        ).toHaveBeenCalledTimes(1)
        const calls = resubmitMutation.mutateAsync.mock.calls
        expect(calls).toHaveLength(2)
        expect(calls[0][0].idempotencyKey).toBe(calls[1][0].idempotencyKey)
        expect(onResult).toHaveBeenLastCalledWith(
            expect.objectContaining({ status: "succeeded" }),
        )
    })

    it("blocks a resolution command whose action differs from resubmit", async () => {
        const ledger = makeLedger()
        ledger.acquire("procurement-rejection-resolution", "sales:so-1:rr", {
            salesOrderId: "so-1",
            action: "VOID_AFTER_REJECTION",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sb-1",
            expectedSalesOrderLockVersion: 2,
            voidReasonCode: "OTHER",
            comment: "",
        } as never)
        const onResult = vi.fn()
        const { result } = renderSubmission({
            commandLedger: ledger,
            initialDraft: makeDraft(),
            purpose: "resubmit",
            onResult,
        })

        await act(async () => {
            result.current.setResubmitEvidence("e-1")
        })
        await act(async () => {
            await expect(result.current.confirmResubmit()).rejects.toThrow(
                "另一项处理的结果仍待确认，请先使用原操作重试。",
            )
        })

        expect(onResult).toHaveBeenCalledWith({
            status: "unknown",
            title: "处理结果待确认",
            description: "另一项处理的结果仍待确认，请先使用原操作重试。",
            reference: "SO-2026-001",
        })
        expect(resubmitMutation.mutateAsync).not.toHaveBeenCalled()
    })
})
