import { describe, it, expect } from "vitest"

import { commandToResultState } from "./result-state"
import type { ProjectionDeliveryCommandResult } from "../types"
import type { ResultState } from "@/components/business/feedback"

function makeResult(
    overrides: Partial<ProjectionDeliveryCommandResult> = {},
): ProjectionDeliveryCommandResult {
    return {
        operationId: "op-1",
        deliveryId: "dlv-1",
        projectionId: "proj-1",
        salesOrderNo: "SO-2026-0001",
        result: "ACKED",
        resultLabel: "已确认",
        occurredAt: "2026-08-01T00:00:00.000Z",
        nextAction: "无需进一步操作",
        stillUnknown: false,
        objectVersion: "3",
        ...overrides,
    }
}

/** 所有命令结果都应产出非空反馈状态。 */
function mapped(
    result: ProjectionDeliveryCommandResult,
): NonNullable<ResultState> {
    const state = commandToResultState(result)
    expect(state).not.toBeNull()
    return state as NonNullable<ResultState>
}

describe("commandToResultState", () => {
    it("STILL_UNKNOWN 映射为结果未知并携带错误中心链接", () => {
        const state = mapped(
            makeResult({
                result: "STILL_UNKNOWN",
                resultLabel: "结果未知",
                stillUnknown: true,
                workItemId: "wi_9",
                errorTaskId: "et_1",
            }),
        )
        expect(state.status).toBe("unknown")
        expect(state.stayUnknown).toBe(true)
        expect(state.reference).toBe("op-1")
        expect(state.w29Href).toBe(
            "/governance/integration-errors?workItemId=wi_9&errorTaskId=et_1&from=W23",
        )
    })

    it("ESCALATED 映射为成功并附错误中心任务事实", () => {
        const state = mapped(
            makeResult({
                result: "ESCALATED",
                resultLabel: "已转人工",
                workItemId: "wi_9",
            }),
        )
        expect(state.status).toBe("succeeded")
        expect(state.title).toBe("已升级到错误中心")
        expect(
            state.facts?.find((f) => f.label === "错误中心任务")?.value,
        ).toBe("wi_9")
    })

    it("FAILED 映射为阻断态", () => {
        const state = mapped(
            makeResult({ result: "FAILED", resultLabel: "失败" }),
        )
        expect(state.status).toBe("blocked")
        expect(state.title).toBe("失败")
    })

    it("ACKED / RETRY_SCHEDULED 映射为成功态并沿用接口文案", () => {
        const acked = mapped(
            makeResult({ result: "ACKED", resultLabel: "已确认" }),
        )
        expect(acked.status).toBe("succeeded")
        expect(acked.title).toBe("已确认")

        const scheduled = mapped(
            makeResult({
                result: "RETRY_SCHEDULED",
                resultLabel: "已安排重试",
                nextAction: "等待后台重试",
            }),
        )
        expect(scheduled.status).toBe("succeeded")
        expect(scheduled.description).toBe("等待后台重试")
    })
})
