import { describe, it, expect } from "vitest"

import {
    computeMetrics,
    filterSummary,
    mapCardForm,
    mapDeliveryStatus,
    mapSource,
    recomputeActions,
    secsToIso,
    toRow,
    type BackendDelivery,
    type BackendProjection,
    type BackendRevision,
} from "./mapping"
import type { ExecutionProjectionRow } from "../types"

describe("secsToIso", () => {
    it("空值与零回落到 epoch", () => {
        expect(secsToIso(undefined)).toBe("1970-01-01T00:00:00.000Z")
        expect(secsToIso(null)).toBe("1970-01-01T00:00:00.000Z")
        expect(secsToIso(0)).toBe("1970-01-01T00:00:00.000Z")
    })

    it("秒时间戳转为 ISO 字符串", () => {
        expect(secsToIso(1_700_000_000)).toBe(
            new Date(1_700_000_000 * 1000).toISOString(),
        )
    })
})

describe("mapDeliveryStatus", () => {
    it.each([
        ["pending_send", "PENDING"],
        ["sending", "SENDING"],
        ["retrying", "RETRYING"],
        ["result_unknown", "UNKNOWN"],
        ["confirmed", "ACKED"],
        ["failed", "FAILED"],
        ["manual", "ESCALATED_MANUAL"],
    ])("映射 %s -> %s", (raw, expected) => {
        expect(mapDeliveryStatus(raw)).toBe(expected)
    })

    it("未知后端状态回落到 PENDING", () => {
        expect(mapDeliveryStatus("future-status")).toBe("PENDING")
    })
})

describe("mapSource", () => {
    it("cutover_snapshot 映射为迁移基线", () => {
        expect(mapSource("cutover_snapshot")).toBe("MIGRATION_BASELINE")
    })

    it("其余来源映射为 ERP 销售版本", () => {
        expect(mapSource("sales_revision")).toBe("ERP_SALES_REVISION")
    })
})

describe("mapCardForm", () => {
    it("electronic/physical 翻译为中文", () => {
        expect(mapCardForm("electronic")).toBe("电子卡")
        expect(mapCardForm("physical")).toBe("实体卡")
    })

    it("其它值原样透传", () => {
        expect(mapCardForm("hybrid")).toBe("hybrid")
    })
})

describe("recomputeActions", () => {
    it("FAILED 允许重试/查询/升级", () => {
        const actions = recomputeActions("FAILED")
        expect(actions.allowedActions).toEqual([
            "RETRY",
            "QUERY_RESULT",
            "ESCALATE",
        ])
        expect(actions.actionBlockers).toHaveLength(0)
    })

    it("PENDING 无动作且给出两条阻断说明", () => {
        const actions = recomputeActions("PENDING")
        expect(actions.allowedActions).toEqual([])
        expect(actions.actionBlockers.map((b) => b.code)).toEqual([
            "NOT_YET_SENT",
            "NO_REQUEST",
        ])
    })

    it("UNKNOWN 只允许查询并阻断重试", () => {
        const actions = recomputeActions("UNKNOWN")
        expect(actions.allowedActions).toEqual(["QUERY_RESULT"])
        expect(actions.actionBlockers.map((b) => b.code)).toEqual([
            "RESULT_UNKNOWN",
        ])
    })

    it("ACKED 阻断重试与查询", () => {
        const actions = recomputeActions("ACKED")
        expect(actions.allowedActions).toEqual([])
        expect(actions.actionBlockers.map((b) => b.code)).toEqual([
            "ALREADY_ACKED",
            "ALREADY_ACKED",
        ])
    })

    it("SENDING 允许查询并阻断重试", () => {
        const actions = recomputeActions("SENDING")
        expect(actions.allowedActions).toEqual(["QUERY_RESULT"])
        expect(actions.actionBlockers.map((b) => b.code)).toEqual(["IN_FLIGHT"])
    })
})

describe("filterSummary", () => {
    it("无筛选时给默认说明", () => {
        expect(filterSummary({})).toBe("默认：风险优先 · 全状态")
    })

    it("拼接全部生效筛选", () => {
        expect(
            filterSummary({
                q: " SO-1 ",
                mallId: "m1",
                deliveryStatus: "FAILED",
                source: "MIGRATION_BASELINE",
                latency: "over_sla",
                reconciliation: "VERSION_MISMATCH",
            }),
        ).toBe(
            "商城=m1 · 状态=FAILED · 来源=迁移基线 · 等待时长=已超时 · 对账=版本差异 · 搜索=SO-1",
        )
    })
})

function makeProjection(
    overrides: Partial<BackendProjection> = {},
): BackendProjection {
    return {
        id: "proj_000000000001",
        sales_order_id: "so_0001",
        target_mall_id: "mall_1",
        version: 1,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

function makeRevision(
    overrides: Partial<BackendRevision> = {},
): BackendRevision {
    return {
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
        ...overrides,
    }
}

function makeDelivery(
    overrides: Partial<BackendDelivery> = {},
): BackendDelivery {
    return {
        id: "dlv_0001",
        projection_revision_id: "rev_0001",
        target_mall_id: "mall_1",
        status: "confirmed",
        attempt_count: 2,
        version: 3,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

describe("toRow", () => {
    it("组装行：投影/修订/投递字段映射与默认值", () => {
        const row = toRow(makeProjection(), makeRevision(), makeDelivery(), [
            { id: "mall_1", name: "测试商城" },
        ])
        expect(row.projectionId).toBe("proj_000000000001")
        expect(row.projectionNo).toBe("PROJ_0000000")
        expect(row.delivery.status).toBe("ACKED")
        expect(row.delivery.statusLabel).toBe("已确认")
        expect(row.delivery.attemptCount).toBe(2)
        expect(row.targetMallName).toBe("测试商城")
        expect(row.objectVersion).toBe("3")
        expect(row.allowedActions).toEqual([])
        expect(row.whitelistPreview).toEqual({
            voucherCategoryErpName: "—",
            faceValue: "100.00",
            cardCount: "3",
            cardForm: "电子卡",
            voucherExpiryAt: "—",
        })
    })

    it("无修订时回落到占位内容与派生投递 ID", () => {
        const row = toRow(makeProjection(), undefined, undefined, [])
        expect(row.projectionRevisionId).toBe("")
        expect(row.delivery.deliveryId).toBe("dlv_proj_000000000001")
        expect(row.delivery.status).toBe("PENDING")
        expect(row.whitelistPreview.faceValue).toBe("0")
    })
})

function rowWithStatus(
    status: ExecutionProjectionRow["delivery"]["status"],
): ExecutionProjectionRow {
    return {
        projectionId: `proj_${status}`,
        projectionNo: `PROJ_${status}`,
        projectionRevisionId: "",
        projectionRevisionNo: 0,
        projectionSource: "ERP_SALES_REVISION",
        salesOrderId: "so_1",
        salesOrderNo: "so_1",
        salesOrderRevisionId: "",
        salesOrderRevisionNo: 0,
        salesOrderStatus: "—",
        salesOrderStatusTone: "neutral",
        customerLabel: "—",
        targetMallId: "mall_1",
        targetMallName: "商城",
        delivery: {
            deliveryId: "dlv_1",
            status,
            statusLabel: "x",
            statusTone: "neutral",
            attemptCount: 0,
        },
        latencyBand: "normal",
        reconciliationStatus: "NONE",
        pendingDurationLabel: "—",
        ownerLabel: "—",
        allowedActions: [],
        actionBlockers: [],
        objectVersion: "1",
        whitelistPreview: {
            voucherCategoryErpName: "—",
            faceValue: "0",
            cardCount: "0",
            cardForm: "—",
            voucherExpiryAt: "—",
        },
    }
}

describe("computeMetrics", () => {
    it("按状态与超时分桶计数", () => {
        const metrics = computeMetrics([
            rowWithStatus("PENDING"),
            rowWithStatus("SENDING"),
            rowWithStatus("UNKNOWN"),
            rowWithStatus("FAILED"),
            rowWithStatus("ESCALATED_MANUAL"),
            rowWithStatus("ACKED"),
        ])
        const byKey = Object.fromEntries(metrics.map((m) => [m.key, m.value]))
        expect(byKey).toEqual({
            pending_send: 1,
            inflight: 2,
            timeout: 0,
            fail_manual: 2,
            acked: 1,
        })
    })
})
