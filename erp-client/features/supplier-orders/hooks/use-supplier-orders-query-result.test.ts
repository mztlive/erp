import { act } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { renderHookWithProviders } from "@/features/test-utils"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"
import { useSupplierOrdersQueryResult } from "./use-supplier-orders-query-result"

const apiMocks = vi.hoisted(() => ({
    fetchSupplierOrderDetail: vi.fn(),
    querySupplierResult: vi.fn(),
}))

vi.mock("@/features/supplier-orders/api/index", () => apiMocks)

function makeRow(
    overrides: Partial<SupplierOrderListRow> = {},
): SupplierOrderListRow {
    return {
        orderId: "so_1",
        orderNo: "SFO-1001",
        mallOrderId: "mall_1",
        mallOrderNo: "MO-1",
        supplierId: "sup_1",
        supplierName: "华东供应商",
        fulfillmentStatus: "RESULT_UNKNOWN",
        fulfillmentLabel: "结果未知",
        fulfillmentTone: "warning",
        cancelStatus: "NONE",
        cancelLabel: "未发起",
        cancelTone: "neutral",
        refundStatus: "NONE",
        refundLabel: "未发起",
        refundTone: "neutral",
        paidAt: "",
        updatedAt: "",
        lastBusinessAt: "",
        itemCount: 1,
        allowedActions: ["OPEN_CENTER", "QUERY_RESULT"],
        actionBlockers: [],
        priority: 100,
        ...overrides,
    }
}

function makeDetail(
    overrides: Partial<SupplierOrderDetailView> = {},
): SupplierOrderDetailView {
    return {
        order: {
            id: "so_1",
            orderNo: "SFO-1001",
            mallOrderId: "mall_1",
            mallOrderNo: "MO-1",
            paidAt: "",
            paymentFactKey: "",
            fulfillmentChain: "ERP_AUTOMATED",
            supplierId: "sup_1",
            supplierName: "华东供应商",
            connectionCode: "conn_1",
            connectionEnvironment: "production",
            supplyVersion: "",
            publicationVersion: "",
            fulfillmentStatus: "RESULT_UNKNOWN",
            fulfillmentLabel: "结果未知",
            fulfillmentTone: "warning",
            cancelStatus: "NONE",
            cancelLabel: "未发起",
            cancelTone: "neutral",
            refundStatus: "NONE",
            refundLabel: "未发起",
            refundTone: "neutral",
            lockVersion: 7,
            paymentOccurredNotice: "商城支付已发生",
        },
        items: [],
        logistics: {},
        statusHistory: [],
        afterSales: [],
        costs: {
            cumulativeCostGross: null,
            cumulativeCostNet: null,
            costSource: "下单成本快照",
        },
        actions: [],
        address: {
            masked: "—",
            phoneMasked: "—",
            recipientMasked: "—",
            canReveal: false,
        },
        placeActionId: "act_1",
        allowedActions: ["OPEN_CENTER", "QUERY_RESULT"],
        actionBlockers: [],
        freshness: { updatedAt: "", state: "fresh" },
        ...overrides,
    }
}

function renderQueryResult() {
    const updateUrl = vi.fn()
    const rendered = renderHookWithProviders(() =>
        useSupplierOrdersQueryResult({ updateUrl }),
    )
    return { ...rendered, updateUrl }
}

beforeEach(() => {
    apiMocks.fetchSupplierOrderDetail.mockReset()
    apiMocks.querySupplierResult.mockReset()
})

describe("useSupplierOrdersQueryResult — list entry", () => {
    it("blocks rows without QUERY_RESULT permission", async () => {
        const { result, updateUrl } = renderQueryResult()
        const row = makeRow({
            allowedActions: ["OPEN_CENTER"],
            actionBlockers: [
                {
                    action: "QUERY_RESULT",
                    code: "NO_QUERY",
                    message: "需先进入任务处理",
                },
            ],
        })

        await act(async () => {
            await result.current.handleQueryFromList(row)
        })

        expect(result.current.actionResult).toEqual({
            status: "blocked",
            title: "无法查询原结果",
            description: "需先进入任务处理",
        })
        expect(updateUrl).not.toHaveBeenCalled()
        expect(apiMocks.querySupplierResult).not.toHaveBeenCalled()
    })

    it("blocks when the detail already carries a formal task", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue(
            makeDetail({ workItem: { workItemId: "wi_1" } as never }),
        )
        const { result, updateUrl } = renderQueryResult()

        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })

        expect(apiMocks.fetchSupplierOrderDetail).toHaveBeenCalledWith({
            orderId: "so_1",
        })
        expect(result.current.actionResult?.status).toBe("blocked")
        expect(result.current.actionResult?.title).toBe("请进入正式任务处理")
        expect(updateUrl).toHaveBeenCalledWith({ preview: "so_1" }, "push")
        expect(apiMocks.querySupplierResult).not.toHaveBeenCalled()
    })

    it("submits an object query with fresh identities and reports success", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue(makeDetail())
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "succeeded",
            message: "已查到原任务处理结果",
            reference: "op_1",
        })
        const { result, updateUrl } = renderQueryResult()

        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })

        expect(apiMocks.querySupplierResult.mock.calls[0]![0]).toEqual({
            commandKind: "OBJECT",
            orderId: "so_1",
            expectedLockVersion: 7,
            action: "QUERY_RESULT",
            targetSupplierActionId: "act_1",
            operationId: expect.stringMatching(/^w26:list-query:/),
            idempotencyKey: expect.stringMatching(/^w26:list-query:/),
        })
        expect(result.current.actionResult).toEqual({
            status: "succeeded",
            title: "查询原结果已完成",
            description: "已查到原任务处理结果",
            reference: "op_1",
        })
        expect(updateUrl).toHaveBeenCalledWith({ preview: "so_1" }, "push")
    })

    it("reports unknown results without a success title", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue(makeDetail())
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "unknown",
            message: "尚未返回",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })

        expect(result.current.actionResult?.status).toBe("unknown")
        expect(result.current.actionResult?.title).toBe("查询结果仍未知")
    })

    it("reports blocked results as 查询未成功", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue(makeDetail())
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "blocked",
            message: "前置条件未满足",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })

        expect(result.current.actionResult).toEqual({
            status: "blocked",
            title: "查询未成功",
            description: "前置条件未满足",
            reference: undefined,
        })
    })

    it("reuses the identity while the result stays unknown", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue(makeDetail())
        apiMocks.querySupplierResult.mockResolvedValue({ status: "unknown" })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })
        await act(async () => {
            await result.current.handleQueryFromList(makeRow())
        })

        const first = apiMocks.querySupplierResult.mock.calls[0]![0]
        const second = apiMocks.querySupplierResult.mock.calls[1]![0]
        expect(second.operationId).toBe(first.operationId)
        expect(second.idempotencyKey).toBe(first.idempotencyKey)
    })
})

describe("useSupplierOrdersQueryResult — preview entry", () => {
    it("submits the preview query with the detail lock version", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "succeeded",
            message: "已查到原任务处理结果",
            reference: "op_2",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.queryFromPreview({
                orderId: "so_1",
                lockVersion: 9,
                placeActionId: "act_2",
            })
        })

        expect(apiMocks.querySupplierResult.mock.calls[0]![0]).toEqual({
            commandKind: "OBJECT",
            orderId: "so_1",
            expectedLockVersion: 9,
            action: "QUERY_RESULT",
            targetSupplierActionId: "act_2",
            operationId: expect.stringMatching(/^w26:preview-query:/),
            idempotencyKey: expect.stringMatching(/^w26:preview-query:/),
        })
        expect(result.current.actionResult?.title).toBe("查询原结果已完成")
    })

    it("maps unknown preview results to the preview wording", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "unknown",
            message: "尚未返回",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.queryFromPreview({
                orderId: "so_1",
                lockVersion: 9,
                placeActionId: "act_2",
            })
        })

        expect(result.current.actionResult).toEqual({
            status: "unknown",
            title: "查询未形成终局成功",
            description: "尚未返回",
            reference: undefined,
        })
    })

    it("maps failed preview results to the failed status", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "failed",
            message: "处理失败",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.queryFromPreview({
                orderId: "so_1",
                lockVersion: 9,
                placeActionId: "act_2",
            })
        })

        expect(result.current.actionResult?.status).toBe("failed")
        expect(result.current.actionResult?.title).toBe("查询未形成终局成功")
    })
})

describe("useSupplierOrdersQueryResult — dismissal", () => {
    it("clears the action result", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({
            status: "succeeded",
            message: "ok",
        })
        const { result } = renderQueryResult()

        await act(async () => {
            await result.current.queryFromPreview({
                orderId: "so_1",
                lockVersion: 1,
                placeActionId: "act_1",
            })
        })
        expect(result.current.actionResult).not.toBeNull()

        act(() => {
            result.current.dismissActionResult()
        })
        expect(result.current.actionResult).toBeNull()
    })
})
