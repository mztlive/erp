import { describe, expect, it } from "vitest"

import {
    newIdempotencyKey,
    outcomeToResult,
} from "@/features/supplier-api-connections/lib/operations"
import type { FormalOutcome } from "@/features/supplier-api-connections/types"

describe("outcomeToResult", () => {
    it("maps succeeded outcomes to a succeeded result with facts", () => {
        const outcome: FormalOutcome = {
            status: "succeeded",
            title: "已创建",
            message: "创建完成",
            reference: "R-1",
            facts: [{ label: "连接代码", value: "CONN-1" }],
        }
        expect(outcomeToResult(outcome)).toEqual({
            status: "succeeded",
            title: "已创建",
            description: "创建完成",
            reference: "R-1",
            facts: [{ label: "连接代码", value: "CONN-1" }],
        })
    })

    it("falls back to the audit event id as reference for succeeded outcomes", () => {
        const outcome: FormalOutcome = {
            status: "succeeded",
            title: "已更新",
            message: "更新完成",
            auditEventId: "evt-9",
        }
        expect(outcomeToResult(outcome)).toEqual({
            status: "succeeded",
            title: "已更新",
            description: "更新完成",
            reference: "evt-9",
            facts: undefined,
        })
    })

    it("maps processing outcomes with job references", () => {
        const outcome: FormalOutcome = {
            status: "processing",
            title: "任务已创建",
            message: "后台执行中",
            jobId: "j1",
            jobNo: "J-1",
        }
        expect(outcomeToResult(outcome)).toEqual({
            status: "processing",
            title: "任务已创建",
            description: "后台执行中",
            reference: "J-1",
            jobId: "j1",
            jobNo: "J-1",
        })
    })

    it("maps unknown outcomes without a reference", () => {
        const outcome: FormalOutcome = {
            status: "unknown",
            title: "结果待确认",
            message: "请查询",
            operationId: "op-1",
            idempotencyKey: "k1",
        }
        expect(outcomeToResult(outcome)).toEqual({
            status: "unknown",
            title: "结果待确认",
            description: "请查询",
        })
    })

    it.each(["rejected", "failed", "blocked"] as const)(
        "maps %s outcomes to their own status with reference",
        (status) => {
            const outcome: FormalOutcome = {
                status,
                code: "X",
                title: "未通过",
                message: "不满足条件",
                reference: "R-2",
            }
            expect(outcomeToResult(outcome)).toEqual({
                status,
                title: "未通过",
                description: "不满足条件",
                reference: "R-2",
            })
        },
    )
})

describe("newIdempotencyKey", () => {
    it("prefixes the key with the given prefix", () => {
        expect(newIdempotencyKey("create")).toMatch(/^create_/)
    })

    it("produces distinct keys across calls", () => {
        const keys = new Set(
            Array.from({ length: 20 }, () => newIdempotencyKey("disable")),
        )
        expect(keys.size).toBe(20)
    })
})
