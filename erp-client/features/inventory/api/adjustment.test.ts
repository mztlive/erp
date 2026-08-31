import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet, apiPost } from "@/lib/api"

import {
    buildCancelStockAdjustmentApprovalRequest,
    cancelStockAdjustmentApproval,
    createAdjustmentDraft,
    resolveAdjustmentUnknown,
    submitAdjustment,
} from "./adjustment"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

const apiPostMock = vi.mocked(apiPost)
const apiGetMock = vi.mocked(apiGet)

const command = {
    expectedVersion: "9007199254740993",
    approvalProcessInstanceId: "approval-instance-1",
    expectedSubjectVersion: "4294967295",
    expectedInstanceVersion: "9007199254740997",
    expectedExecutionVersion: "9007199254740999",
    expectedTaskVersion: "9007199254741001",
} as const

describe("stock adjustment approval cancellation", () => {
    beforeEach(() => {
        apiPostMock.mockReset()
        apiGetMock.mockReset()
    })

    it("keeps every CAS version as the exact string supplied by the detail token", () => {
        expect(
            buildCancelStockAdjustmentApprovalRequest({
                command,
                reason: "  需要修改数量  ",
                idempotencyKey: "cancel-intent-1",
            }),
        ).toEqual({
            expected_version: "9007199254740993",
            approval_process_instance_id: "approval-instance-1",
            expected_subject_version: "4294967295",
            expected_instance_version: "9007199254740997",
            expected_execution_version: "9007199254740999",
            expected_task_version: "9007199254741001",
            reason: "需要修改数量",
            idempotency_key: "cancel-intent-1",
        })
    })

    it("uses the stock adjustment resource endpoint and maps StockAdjustmentView", async () => {
        apiPostMock.mockResolvedValue({
            id: "adjustment/1",
            adjustment_no: "TZ-1",
            warehouse_id: "warehouse-1",
            reason_type: "STOCK_LOSS",
            status: "DRAFT",
            prepared_by: "user-1",
            version: "4",
            created_at: 1,
        })

        const result = await cancelStockAdjustmentApproval({
            stockAdjustmentId: "adjustment/1",
            command,
            reason: "需要修改数量",
            idempotencyKey: "cancel-intent-1",
        })

        expect(apiPostMock).toHaveBeenCalledWith(
            "/admin/stock-adjustments/adjustment%2F1/cancel-approval",
            expect.objectContaining({
                approval_process_instance_id: "approval-instance-1",
                idempotency_key: "cancel-intent-1",
            }),
        )
        expect(result).toEqual({
            stockAdjustmentId: "adjustment/1",
            status: "DRAFT",
        })
        expect(result).not.toHaveProperty("instanceId")
    })

    it("creates a draft with the exact string balance version", async () => {
        apiPostMock.mockResolvedValue({
            adjustment: {
                id: "adjustment-1",
                adjustment_no: "TZ-1",
                warehouse_id: "warehouse-1",
                reason_type: "STOCK_LOSS",
                status: "DRAFT",
                prepared_by: "user-1",
                version: "4",
                created_at: 1,
            },
            lines: [
                {
                    id: "line-1",
                    sku_id: "sku-1",
                    quantity: "1",
                    direction: "DECREASE",
                },
            ],
            posted_movements: [],
            approval: {
                requirement: "PROCESS_REQUIRED",
                recent_history: [],
                history_page: { items: [], has_more: false },
                allowed_actions: ["SUBMIT"],
                submit_command: {
                    expected_version: "4",
                    expected_subject_version: "2",
                },
            },
        })

        const result = await createAdjustmentDraft({
            balanceId: "balance-1",
            balanceLockVersion: "9007199254741031",
            warehouseId: "warehouse-1",
            warehouseName: "一号仓",
            skuId: "sku-1",
            skuCode: "SKU-1",
            skuName: "商品一",
            baseUnit: "件",
        })

        expect(apiPostMock).toHaveBeenCalledWith(
            "/admin/stock-adjustments",
            expect.objectContaining({
                expected_balance_version: "9007199254741031",
            }),
        )
        expect(result.balanceLockVersion).toBe("9007199254741031")
        expect(result.approval?.submitCommand).toEqual({
            expectedVersion: "4",
            expectedSubjectVersion: "2",
        })
    })

    it("submits the server-issued document and subject versions without deriving them", async () => {
        apiPostMock.mockResolvedValue({
            adjustment: {
                id: "adjustment-1",
                adjustment_no: "TZ-1",
                warehouse_id: "warehouse-1",
                reason_type: "STOCK_LOSS",
                status: "IN_APPROVAL",
                prepared_by: "user-1",
                version: "4",
                created_at: 1,
            },
            lines: [],
            posted_movements: [],
            approval: {
                requirement: "PROCESS_REQUIRED",
                recent_history: [],
                history_page: { items: [], has_more: false },
                allowed_actions: [],
            },
        })

        const input = {
            stockAdjustmentId: "adjustment-1",
            submitCommand: {
                expectedVersion: "9007199254741011",
                expectedSubjectVersion: "4294967294",
            },
            balanceId: "balance-1",
            lineId: "line-1",
            expectedBalanceLockVersion: "9007199254741031",
            reasonType: "COUNT_LOSS" as const,
            reasonTypeLabel: "盘亏",
            direction: "decrease" as const,
            quantity: "2",
            note: "盘点差异",
            occurredAt: "2026-09-01T10:00:00.000Z",
            idempotencyKey: "submit-intent-1",
        }

        await submitAdjustment(input)

        expect(apiPostMock).toHaveBeenCalledWith(
            "/admin/stock-adjustments/adjustment-1/submit",
            expect.objectContaining({
                expected_version: "9007199254741011",
                expected_subject_version: "4294967294",
                balances: [
                    {
                        balance_id: "balance-1",
                        expected_version: "9007199254741031",
                    },
                ],
                idempotency_key: "submit-intent-1",
            }),
        )
        const firstRequest = apiPostMock.mock.calls[0]?.[1]
        await submitAdjustment(input)
        const retryRequest = apiPostMock.mock.calls[1]?.[1] as Record<
            string,
            unknown
        >
        expect(retryRequest).toEqual(firstRequest)
        expect(retryRequest["idempotency_key"]).toBe("submit-intent-1")
        expect(retryRequest["expected_subject_version"]).toBe("4294967294")
    })

    it.each(["APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT", "VERSION_CONFLICT"])(
        "preserves an exact 409 code without inventing a lock version: %s",
        async (code) => {
            apiPostMock.mockRejectedValue({
                kind: "Http",
                status: 409,
                code,
                message: "当前请求冲突",
            })

            const result = await submitAdjustment({
                stockAdjustmentId: "adjustment-1",
                submitCommand: {
                    expectedVersion: "9007199254741011",
                    expectedSubjectVersion: "4294967294",
                },
                balanceId: "balance-1",
                lineId: "line-1",
                expectedBalanceLockVersion: "9007199254741031",
                reasonType: "COUNT_LOSS",
                reasonTypeLabel: "盘亏",
                direction: "decrease",
                quantity: "2",
                note: "盘点差异",
                occurredAt: "2026-09-01T10:00:00.000Z",
                idempotencyKey: "submit-intent-1",
            })

            expect(result).toMatchObject({ status: "failed", code })
            expect(result).not.toHaveProperty("latestLockVersion")
        },
    )

    it("classifies OUTCOME_UNKNOWN before the generic HTTP 409 branch", async () => {
        apiPostMock.mockRejectedValue({
            kind: "Http",
            status: 409,
            code: "OUTCOME_UNKNOWN",
            message: "操作结果暂时无法确认",
        })

        const result = await submitAdjustment({
            stockAdjustmentId: "adjustment-1",
            submitCommand: {
                expectedVersion: "4",
                expectedSubjectVersion: "2",
            },
            balanceId: "balance-1",
            lineId: "line-1",
            expectedBalanceLockVersion: "7",
            reasonType: "COUNT_LOSS",
            reasonTypeLabel: "盘亏",
            direction: "decrease",
            quantity: "2",
            note: "盘点差异",
            occurredAt: "2026-09-01T10:00:00.000Z",
            idempotencyKey: "submit-intent-unknown",
        })

        expect(result).toEqual({
            status: "unknown",
            message: "操作结果暂时无法确认",
            idempotencyKey: "submit-intent-unknown",
        })
    })

    it("resolves an unknown result only through the exact receipt lookup", async () => {
        apiGetMock.mockResolvedValue({
            adjustment: {
                id: "adjustment/1",
                adjustment_no: "TZ-1",
                warehouse_id: "warehouse-1",
                reason_type: "STOCK_LOSS",
                status: "IN_APPROVAL",
                prepared_by: "user-1",
                version: "4",
                created_at: 1,
            },
            lines: [],
            posted_movements: [],
            approval: {
                requirement: "PROCESS_REQUIRED",
                recent_history: [],
                history_page: { items: [], has_more: false },
                allowed_actions: [],
            },
        })

        const result = await resolveAdjustmentUnknown({
            stockAdjustmentId: "adjustment/1",
            expectedSubjectVersion: "4294967294",
            expectedBalanceLockVersion: "9007199254741031",
            idempotencyKey: "submit intent/1",
        })

        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/stock-adjustments/adjustment%2F1/submit-result?expected_subject_version=4294967294&idempotency_key=submit+intent%2F1",
        )
        expect(result).toMatchObject({
            status: "succeeded",
            outcome: {
                stockAdjustmentId: "adjustment/1",
                balanceLockVersion: "9007199254741031",
            },
        })
    })

    it("does not infer success from the current document status when no exact receipt exists", async () => {
        apiGetMock.mockRejectedValue({
            kind: "Http",
            status: 404,
            code: "STOCK_ADJUSTMENT_SUBMIT_RESULT_NOT_FOUND",
            message: "未找到原命令结果",
        })

        const result = await resolveAdjustmentUnknown({
            stockAdjustmentId: "adjustment-1",
            expectedSubjectVersion: "2",
            idempotencyKey: "wrong-key",
        })

        expect(result).toEqual({
            status: "failed",
            code: "NO_PENDING",
            message: "未找到该任务号对应的处理中请求",
        })
        expect(apiGetMock).toHaveBeenCalledTimes(1)
    })

    it("keeps the outcome unknown when the exact receipt lookup is unavailable", async () => {
        apiGetMock.mockRejectedValue({
            kind: "Http",
            status: 503,
            code: "SERVICE_UNAVAILABLE",
            message: "系统暂时无法完成查询",
        })

        const result = await resolveAdjustmentUnknown({
            stockAdjustmentId: "adjustment-1",
            expectedSubjectVersion: "2",
            idempotencyKey: "submit-intent-1",
        })

        expect(result).toEqual({
            status: "unknown",
            message: "系统暂时无法完成查询",
            idempotencyKey: "submit-intent-1",
        })
    })
})
