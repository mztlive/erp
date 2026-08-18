import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    useSalesOrderDetailRejectionResolution,
    useSalesOrderDetailStartChange,
} from "@/features/sales-orders/hooks/use-sales-order-detail-commands"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { FormalCommandKeyLedger } from "@/lib/formal-command"

const apiMocks = vi.hoisted(() => ({
    prepareProcurementRejectionResolution: vi.fn(),
    prepareStartSalesChangeOrder: vi.fn(),
    resolveProcurementRejection: vi.fn(),
    startSalesChangeOrder: vi.fn(),
}))

vi.mock("@/features/sales-orders/api/sales-orders", async (importOriginal) => {
    const actual =
        await importOriginal<
            typeof import("@/features/sales-orders/api/sales-orders")
        >()
    return {
        ...actual,
        prepareProcurementRejectionResolution:
            apiMocks.prepareProcurementRejectionResolution,
        prepareStartSalesChangeOrder: apiMocks.prepareStartSalesChangeOrder,
        resolveProcurementRejection: apiMocks.resolveProcurementRejection,
        startSalesChangeOrder: apiMocks.startSalesChangeOrder,
    }
})

function makeOrder(
    overrides: Partial<SalesOrderDetailView> = {},
): SalesOrderDetailView {
    return {
        id: "so-1",
        documentNumber: "XS-1",
        version: 3,
        nature: "physical_service",
        ...overrides,
    } as unknown as SalesOrderDetailView
}

let keySeq = 0
function freshLedger() {
    keySeq = 0
    return new FormalCommandKeyLedger(() => `key-${++keySeq}`)
}

function freshOnResult() {
    return vi.fn<(next: SalesOrderDetailActionResult) => void>()
}

const preparedVoidPayload = {
    salesOrderId: "so-1",
    action: "VOID_AFTER_REJECTION" as const,
    voidReasonCode: "SALES_DECISION_NOT_TO_PROCEED",
    comment: "客户不再需要",
    rejectedProcurementConfirmationId: "pc-1",
    rejectedSubmissionId: "sub-1",
    expectedSalesOrderLockVersion: 3,
}

const voidOutcome = {
    outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION" as const,
    reference: "XS-1",
    detail: "本单已作废。",
}

describe("useSalesOrderDetailRejectionResolution", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        keySeq = 0
    })

    it("voids after rejection with a prepared command and settles the ledger", async () => {
        apiMocks.prepareProcurementRejectionResolution.mockResolvedValue(
            preparedVoidPayload,
        )
        apiMocks.resolveProcurementRejection.mockResolvedValue(voidOutcome)
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()

        await act(async () => {
            await result.current.voidAfterRejection({
                order: makeOrder(),
                commandLedger: ledger,
                onResult,
                reason: "客户不再需要",
            })
        })

        expect(
            apiMocks.prepareProcurementRejectionResolution,
        ).toHaveBeenCalledWith({
            salesOrderId: "so-1",
            action: "VOID_AFTER_REJECTION",
            voidReasonCode: "SALES_DECISION_NOT_TO_PROCEED",
            comment: "客户不再需要",
        })
        expect(apiMocks.resolveProcurementRejection).toHaveBeenCalledWith(
            {
                ...preparedVoidPayload,
                idempotencyKey: "key-1",
            },
            expect.anything(),
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "rejected",
            title: "本单已作废",
            description: "本单已作废。",
            reference: "XS-1",
        })
        expect(ledger.peek("procurement-rejection-resolution")).toBeUndefined()
    })

    it("rejects validation of the low-margin request before any call", async () => {
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()

        await act(async () => {
            await expect(
                result.current.requestLowMargin({
                    order: makeOrder(),
                    commandLedger: ledger,
                    onResult,
                    reason: "   ",
                    evidence: "EV-1",
                }),
            ).rejects.toThrow("请填写低毛利承接理由")
            await expect(
                result.current.requestLowMargin({
                    order: makeOrder(),
                    commandLedger: ledger,
                    onResult,
                    reason: "维持原价",
                    evidence: "   ",
                }),
            ).rejects.toThrow("请至少填写一项已登记证据 ID")
        })

        expect(
            apiMocks.prepareProcurementRejectionResolution,
        ).not.toHaveBeenCalled()
        expect(onResult).not.toHaveBeenCalled()
    })

    it("requests low-margin acceptance with trimmed reason and parsed evidence", async () => {
        apiMocks.prepareProcurementRejectionResolution.mockResolvedValue({
            salesOrderId: "so-1",
            action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
            lowMarginAcceptanceReason: "维持原价",
            evidenceReferenceIds: ["EV-1", "EV-2", "EV-3"],
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sub-1",
            expectedSalesOrderLockVersion: 3,
        })
        apiMocks.resolveProcurementRejection.mockResolvedValue({
            outcome: "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED",
            reference: "PC-9",
            detail: "已创建低毛利确认。",
        })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()

        await act(async () => {
            await result.current.requestLowMargin({
                order: makeOrder(),
                commandLedger: ledger,
                onResult,
                reason: "  维持原价  ",
                evidence: "EV-1, EV-2，EV-1;EV-3",
            })
        })

        expect(
            apiMocks.prepareProcurementRejectionResolution,
        ).toHaveBeenCalledWith({
            salesOrderId: "so-1",
            action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
            lowMarginAcceptanceReason: "维持原价",
            evidenceReferenceIds: ["EV-1", "EV-2", "EV-3"],
        })
        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "已申请低毛利承接",
            description: "已创建低毛利确认。",
            reference: "PC-9",
            nextResponsible: "销售上级",
        })
        expect(ledger.peek("procurement-rejection-resolution")).toBeUndefined()
    })

    it("blocks a conflicting command and keeps the ledger untouched", async () => {
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )
        const ledger = freshLedger()
        ledger.acquire(
            "procurement-rejection-resolution",
            "sales:so-1:procurement-rejection:low-margin",
            {
                salesOrderId: "so-1",
                action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
                lowMarginAcceptanceReason: "维持原价",
                evidenceReferenceIds: ["EV-1"],
                rejectedProcurementConfirmationId: "pc-1",
                rejectedSubmissionId: "sub-1",
                expectedSalesOrderLockVersion: 3,
            },
        )
        const onResult = freshOnResult()

        await act(async () => {
            await expect(
                result.current.voidAfterRejection({
                    order: makeOrder(),
                    commandLedger: ledger,
                    onResult,
                    reason: "作废",
                }),
            ).rejects.toThrow("另一项处理的结果仍待确认，请先使用原操作重试。")
        })

        expect(onResult).toHaveBeenCalledWith({
            status: "unknown",
            title: "处理结果待确认",
            description: "另一项处理的结果仍待确认，请先使用原操作重试。",
            reference: "XS-1",
        })
        expect(
            apiMocks.prepareProcurementRejectionResolution,
        ).not.toHaveBeenCalled()
    })

    it("keeps the command on an unknown outcome and retries with the same key", async () => {
        apiMocks.prepareProcurementRejectionResolution.mockResolvedValue(
            preparedVoidPayload,
        )
        apiMocks.resolveProcurementRejection
            .mockRejectedValueOnce({ kind: "Network", message: "offline" })
            .mockResolvedValueOnce(voidOutcome)
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()
        const order = makeOrder()

        await act(async () => {
            await expect(
                result.current.voidAfterRejection({
                    order,
                    commandLedger: ledger,
                    onResult,
                    reason: "客户不再需要",
                }),
            ).rejects.toEqual({ kind: "Network", message: "offline" })
        })

        expect(onResult).toHaveBeenLastCalledWith({
            status: "unknown",
            title: "处理结果待确认",
            description: "当前原因已保留，请使用本次操作重试。",
            reference: "XS-1",
        })
        expect(
            apiMocks.prepareProcurementRejectionResolution,
        ).toHaveBeenCalledTimes(1)

        await act(async () => {
            await result.current.voidAfterRejection({
                order,
                commandLedger: ledger,
                onResult,
                reason: "客户不再需要",
            })
        })

        expect(apiMocks.resolveProcurementRejection).toHaveBeenCalledTimes(2)
        expect(apiMocks.resolveProcurementRejection).toHaveBeenNthCalledWith(
            2,
            {
                ...preparedVoidPayload,
                idempotencyKey: "key-1",
            },
            expect.anything(),
        )
        expect(onResult).toHaveBeenLastCalledWith({
            status: "rejected",
            title: "本单已作废",
            description: "本单已作废。",
            reference: "XS-1",
        })
        expect(ledger.peek("procurement-rejection-resolution")).toBeUndefined()
    })

    it("exposes isPending while the mutation is in flight", async () => {
        let release!: (value: typeof voidOutcome) => void
        apiMocks.prepareProcurementRejectionResolution.mockResolvedValue(
            preparedVoidPayload,
        )
        apiMocks.resolveProcurementRejection.mockReturnValue(
            new Promise((resolve) => {
                release = resolve
            }),
        )
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailRejectionResolution(),
        )

        let pending: Promise<unknown>
        await act(async () => {
            pending = result.current
                .voidAfterRejection({
                    order: makeOrder(),
                    commandLedger: freshLedger(),
                    onResult: freshOnResult(),
                    reason: "客户不再需要",
                })
                .catch(() => {})
            await vi.waitFor(() => expect(result.current.isPending).toBe(true))
        })

        await act(async () => {
            release(voidOutcome)
            await pending
        })
        await waitFor(() => expect(result.current.isPending).toBe(false))
    })
})

describe("useSalesOrderDetailStartChange", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        keySeq = 0
    })

    it("starts a change order without inventing the next approver", async () => {
        apiMocks.prepareStartSalesChangeOrder.mockResolvedValue({
            salesOrderId: "so-1",
            baseRevisionNo: 3,
            nature: "physical_service",
            command: { draft: {} },
        })
        apiMocks.startSalesChangeOrder.mockResolvedValue({
            id: "sc-1",
            statusLabel: "待财务复核",
            statusTone: "warning",
            baseRevisionNo: 3,
            createdAt: "2026-08-14T00:00:00.000Z",
            impactPath: "procurement",
        })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailStartChange(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()

        await act(async () => {
            await result.current.startChange({
                order: makeOrder(),
                commandLedger: ledger,
                onResult,
            })
        })

        expect(apiMocks.prepareStartSalesChangeOrder).toHaveBeenCalledWith({
            salesOrderId: "so-1",
            baseRevisionNo: 3,
            nature: "physical_service",
        })
        expect(apiMocks.startSalesChangeOrder).toHaveBeenCalledWith(
            {
                salesOrderId: "so-1",
                baseRevisionNo: 3,
                nature: "physical_service",
                command: { draft: {} },
                idempotencyKey: "key-1",
            },
            expect.anything(),
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "改单已创建",
            description: "已进入「待财务复核」。当前版本对客户仍然有效。",
            reference: "sc-1",
            nextResponsible: undefined,
        })
        expect(ledger.peek("start-change")).toBeUndefined()
    })

    it("does not invent operations or finance as the next approver for card orders", async () => {
        apiMocks.prepareStartSalesChangeOrder.mockResolvedValue({
            salesOrderId: "so-1",
            baseRevisionNo: 3,
            nature: "card_voucher",
            command: { draft: {} },
        })
        apiMocks.startSalesChangeOrder.mockResolvedValue({
            id: "sc-2",
            statusLabel: "待运营执行影响确认",
            statusTone: "warning",
            baseRevisionNo: 3,
            createdAt: "2026-08-14T00:00:00.000Z",
            impactPath: "operations",
        })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailStartChange(),
        )
        const onResult = freshOnResult()

        await act(async () => {
            await result.current.startChange({
                order: makeOrder({ nature: "card_voucher" }),
                commandLedger: freshLedger(),
                onResult,
            })
        })

        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "改单已创建",
            description:
                "已进入「待运营执行影响确认」。当前版本对客户仍然有效。",
            reference: "sc-2",
            nextResponsible: undefined,
        })
    })

    it("keeps the command on an unknown outcome and retries with the same key", async () => {
        apiMocks.prepareStartSalesChangeOrder.mockResolvedValue({
            salesOrderId: "so-1",
            baseRevisionNo: 3,
            nature: "physical_service",
            command: { draft: {} },
        })
        apiMocks.startSalesChangeOrder
            .mockRejectedValueOnce({ kind: "Network", message: "offline" })
            .mockResolvedValueOnce({
                id: "sc-1",
                statusLabel: "待财务复核",
                statusTone: "warning",
                baseRevisionNo: 3,
                createdAt: "2026-08-14T00:00:00.000Z",
                impactPath: "procurement",
            })
        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailStartChange(),
        )
        const ledger = freshLedger()
        const onResult = freshOnResult()
        const order = makeOrder()

        await act(async () => {
            await expect(
                result.current.startChange({
                    order,
                    commandLedger: ledger,
                    onResult,
                }),
            ).rejects.toEqual({ kind: "Network", message: "offline" })
        })

        expect(onResult).toHaveBeenLastCalledWith({
            status: "unknown",
            title: "处理结果待确认",
            description: "请使用本次操作重试；确认前不要重复创建改单。",
            reference: "XS-1",
        })

        await act(async () => {
            await result.current.startChange({
                order,
                commandLedger: ledger,
                onResult,
            })
        })

        expect(apiMocks.prepareStartSalesChangeOrder).toHaveBeenCalledTimes(1)
        expect(apiMocks.startSalesChangeOrder).toHaveBeenCalledTimes(2)
        expect(apiMocks.startSalesChangeOrder).toHaveBeenNthCalledWith(
            2,
            {
                salesOrderId: "so-1",
                baseRevisionNo: 3,
                nature: "physical_service",
                command: { draft: {} },
                idempotencyKey: "key-1",
            },
            expect.anything(),
        )
        expect(ledger.peek("start-change")).toBeUndefined()
    })
})
