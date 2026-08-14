import { describe, it, expect, vi, beforeEach } from "vitest"

vi.mock("@/lib/api", async (importOriginal) => {
    const mod = await importOriginal<typeof import("@/lib/api")>()
    return { ...mod, apiGet: vi.fn(), apiPost: vi.fn() }
})

import { apiGet, apiPost } from "@/lib/api"
import {
    fetchExecutionProjectionDetail,
    fetchExecutionProjectionList,
    fetchSalesOrderCollaboration,
    submitBulkProjectionCommand,
    submitProjectionDeliveryCommand,
} from "./projections"
import type {
    BackendDelivery,
    BackendProjection,
    BackendRevision,
} from "./mapping"

const mockedApiGet = vi.mocked(apiGet)
const mockedApiPost = vi.mocked(apiPost)

const mallsPage = {
    items: [
        { id: "mall_1", code: "M1", name: "测试商城" },
        { id: "mall_2", code: "M2", name: "二号商城" },
    ],
    total: 2,
    page: 1,
    page_size: 100,
}

const projection: BackendProjection = {
    id: "proj_000000000001",
    sales_order_id: "so_0001",
    target_mall_id: "mall_1",
    version: 1,
    created_at: 1_700_000_000,
}

const revision: BackendRevision = {
    id: "rev_0001",
    projection_id: "proj_000000000001",
    revision_no: 2,
    projection_source: "sales_revision",
    sales_order_revision_id: "sor_0001",
    customer_external_identity: "CUST-9",
    face_value: "100.00",
    card_count: 3,
    card_form: "electronic",
    effective_at: 1_700_000_000,
    version: 1,
    created_at: 1_700_000_000,
}

const delivery: BackendDelivery = {
    id: "dlv_0001",
    projection_revision_id: "rev_0001",
    target_mall_id: "mall_1",
    status: "confirmed",
    attempt_count: 2,
    mall_ack_at: 1_700_000_100,
    version: 3,
    created_at: 1_700_000_000,
}

function emptyPage() {
    return { items: [], total: 0, page: 1, page_size: 100 }
}

function stubListBackend() {
    mockedApiGet.mockImplementation((path: string) => {
        if (path === "/admin/source-systems") {
            return Promise.resolve(mallsPage)
        }
        if (path === "/admin/sales-order-projections") {
            return Promise.resolve({
                items: [projection],
                total: 1,
                page: 1,
                page_size: 20,
            })
        }
        if (path === "/admin/sales-order-projections/proj_000000000001") {
            return Promise.resolve(projection)
        }
        if (path === "/admin/sales-order-projection-deliveries") {
            return Promise.resolve({
                items: [delivery],
                total: 1,
                page: 1,
                page_size: 100,
            })
        }
        if (path.endsWith("/revisions")) {
            return Promise.resolve([revision])
        }
        return Promise.resolve(emptyPage())
    })
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("fetchExecutionProjectionList", () => {
    it("请求投影/投递/修订并映射为行、指标与筛选摘要", async () => {
        stubListBackend()
        const result = await fetchExecutionProjectionList({
            page: 2,
            pageSize: 20,
            mallId: "mall_1",
        })

        expect(result.rows).toHaveLength(1)
        const row = result.rows[0]!
        expect(row.projectionId).toBe("proj_000000000001")
        expect(row.projectionNo).toBe("PROJ_0000000")
        expect(row.delivery.status).toBe("ACKED")
        expect(row.delivery.attemptCount).toBe(2)
        expect(row.targetMallName).toBe("测试商城")
        expect(result.malls).toEqual([
            { id: "mall_1", name: "测试商城" },
            { id: "mall_2", name: "二号商城" },
        ])
        expect(result.pageInfo).toEqual({ page: 1, pageSize: 20, total: 1 })
        expect(result.metrics.find((m) => m.key === "acked")?.value).toBe(1)
        expect(result.filterSummary).toBe("商城=mall_1")

        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/sales-order-projections",
            {
                page: 2,
                page_size: 20,
                sort_by: "updated_at",
                sort_dir: "desc",
                target_mall_id: "mall_1",
            },
        )
    })

    it("q 筛选按单号/客户/商城大小写不敏感匹配", async () => {
        stubListBackend()
        const hit = await fetchExecutionProjectionList({ q: "cust-9" })
        expect(hit.rows).toHaveLength(1)

        const miss = await fetchExecutionProjectionList({ q: "不存在" })
        expect(miss.rows).toHaveLength(0)
        expect(miss.filterSummary).toBe("搜索=不存在")
    })

    it("metric=acked 只保留已确认行", async () => {
        stubListBackend()
        const result = await fetchExecutionProjectionList({ metric: "acked" })
        expect(result.rows).toHaveLength(1)

        const filtered = await fetchExecutionProjectionList({
            metric: "pending_send",
        })
        expect(filtered.rows).toHaveLength(0)
    })
})

describe("fetchExecutionProjectionDetail", () => {
    it("404 时返回 null 而不抛错", async () => {
        mockedApiGet.mockImplementation((path: string) => {
            if (path === "/admin/source-systems")
                return Promise.resolve(mallsPage)
            return Promise.reject({ kind: "Http", status: 404 })
        })
        await expect(
            fetchExecutionProjectionDetail({ projectionId: "missing" }),
        ).resolves.toBeNull()
    })

    it("组装身份、三轨、投递顺序与修订链", async () => {
        stubListBackend()
        const detail = await fetchExecutionProjectionDetail({
            projectionId: "proj_000000000001",
        })
        expect(detail?.identity.projectionNo).toBe("PROJ_0000000")
        expect(detail?.identity.targetMallName).toBe("测试商城")
        expect(detail?.tracks.projectionDelivery.label).toBe("已确认")
        expect(detail?.tracks.mallConfirm.label).toBe("已确认")
        expect(detail?.selectedRevision.content.faceValue).toBe("100.00")
        expect(detail?.deliveries).toHaveLength(1)
        expect(detail?.revisionLinks).toHaveLength(1)
        expect(detail?.revisionLinks[0]?.isCurrentSelection).toBe(true)
        expect(detail?.objectVersion).toBe("3")
    })
})

describe("fetchSalesOrderCollaboration", () => {
    it("无投影时返回未形成摘要", async () => {
        mockedApiGet.mockImplementation((path: string) => {
            if (path === "/admin/sales-order-projections")
                return Promise.resolve(emptyPage())
            return Promise.resolve(mallsPage)
        })
        const summary = await fetchSalesOrderCollaboration("so_0001")
        expect(summary.hasProjection).toBe(false)
        expect(summary.historyCount).toBe(0)
    })

    it("有投影时携带详情摘要与入口链接", async () => {
        stubListBackend()
        const summary = await fetchSalesOrderCollaboration("so_0001")
        expect(summary.hasProjection).toBe(true)
        expect(summary.projectionNo).toBe("PROJ_0000000")
        expect(summary.delivery?.status).toBe("ACKED")
        expect(summary.w23Href).toBe(
            "/commerce/execution-projections?projectionId=proj_000000000001",
        )
    })
})

describe("submitProjectionDeliveryCommand", () => {
    it("映射后端动作结果并保留 unknown 语义", async () => {
        mockedApiPost.mockResolvedValue({
            operation_id: "op_1",
            delivery_id: "dlv_0001",
            result: "STILL_UNKNOWN",
            work_item_id: "wi_9",
            occurred_at: 1_700_000_100,
            next_action: "再次查询",
        })
        const result = await submitProjectionDeliveryCommand({
            projectionId: "proj_000000000001",
            projectionRevisionId: "rev_0001",
            deliveryId: "dlv_0001",
            action: "QUERY_RESULT",
            expectedObjectVersion: "3",
            requestId: "req-1",
        })
        expect(result.stillUnknown).toBe(true)
        expect(result.resultLabel).toBe("结果未知")
        expect(result.workItemId).toBe("wi_9")
        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/sales-order-projection-deliveries/dlv_0001/actions",
            {
                projection_id: "proj_000000000001",
                projection_revision_id: "rev_0001",
                delivery_id: "dlv_0001",
                action: "QUERY_RESULT",
                expected_object_version: 3,
                request_id: "req-1",
            },
        )
    })
})

describe("submitBulkProjectionCommand", () => {
    it("空选择返回失败任务且不发请求", async () => {
        const job = await submitBulkProjectionCommand({
            action: "BULK_QUERY",
            projectionIds: [],
            requestId: "r1",
        })
        expect(job.status).toBe("failed")
        expect(job.total).toBe(0)
        expect(job.items).toEqual([])
        expect(mockedApiGet).not.toHaveBeenCalled()
        expect(mockedApiPost).not.toHaveBeenCalled()
    })

    it("超过批量上限的任务整体拒绝并逐项给出原因", async () => {
        const ids = Array.from({ length: 21 }, (_, i) => `p_${i}`)
        const job = await submitBulkProjectionCommand({
            action: "BULK_RETRY",
            projectionIds: ids,
            requestId: "r2",
        })
        expect(job.status).toBe("failed")
        expect(job.total).toBe(21)
        expect(job.failed).toBe(21)
        expect(job.items.every((i) => i.outcome === "failed")).toBe(true)
        expect(mockedApiGet).not.toHaveBeenCalled()
    })

    it("逐项成功后汇总为 succeeded 任务", async () => {
        stubListBackend()
        mockedApiPost.mockResolvedValue({
            operation_id: "op_1",
            delivery_id: "dlv_0001",
            result: "ACKED",
            occurred_at: 1_700_000_100,
        })
        const job = await submitBulkProjectionCommand({
            action: "BULK_QUERY",
            projectionIds: ["proj_000000000001"],
            requestId: "r3",
        })
        expect(job.status).toBe("succeeded")
        expect(job.succeeded).toBe(1)
        expect(job.failed).toBe(0)
        expect(job.stillUnknown).toBe(0)
        expect(job.items[0]?.outcome).toBe("succeeded")
    })
})
