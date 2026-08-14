import { describe, expect, it } from "vitest"

import { accessChangeResultState } from "./outcome-state"

describe("accessChangeResultState", () => {
    it("maps a CONFIRMED outcome to a succeeded result with facts", () => {
        const result = accessChangeResultState({
            outcome: "CONFIRMED",
            permissionVersion: "pv-live-7",
            auditEventId: "ae_1",
            affectedSubjectCount: 1,
            effectiveAt: "2026-01-01T00:00:00.000Z",
            reference: "op_1",
            nextSteps: ["刷新用户授权列表", "核对有效权限解释"],
            message: "已提交紧急撤权。",
        })
        expect(result?.status).toBe("succeeded")
        expect(result?.title).toBe("授权变更已生效")
        expect(result?.reference).toBe("op_1")
        expect(result?.facts).toEqual([
            { label: "配置版本", value: "v7" },
            { label: "影响主体数", value: "1" },
            { label: "审计事件号", value: "ae_1" },
            {
                label: "生效时间",
                value: expect.stringContaining("2026"),
            },
            { label: "下一步", value: "刷新用户授权列表；核对有效权限解释" },
        ])
    })

    it("maps a REJECTED outcome without policy to a blocked result", () => {
        const result = accessChangeResultState({
            outcome: "REJECTED",
            code: "REVIEW_POLICY_UNCONFIGURED",
            message: "复核策略未确定",
            actionBlockers: [
                {
                    action: "DISABLE_ROLE",
                    code: "REVIEW_POLICY_UNCONFIGURED",
                    message: "复核策略未确定",
                },
            ],
        })
        expect(result?.status).toBe("blocked")
        expect(result?.title).toBe("复核策略未确定，动作已阻断")
        expect(result?.facts).toEqual([
            {
                label: "REVIEW_POLICY_UNCONFIGURED",
                value: "复核策略未确定",
            },
        ])
    })

    it("maps other REJECTED outcomes to a rejected result", () => {
        const result = accessChangeResultState({
            outcome: "REJECTED",
            code: "UNSUPPORTED_COMMAND",
            message: "未映射到后端 HTTP 写路径。",
        })
        expect(result?.status).toBe("rejected")
        expect(result?.title).toBe("授权变更被拒绝")
        expect(result?.facts).toBeUndefined()
    })

    it("maps a CONFLICT outcome to a blocked result with the server version", () => {
        const result = accessChangeResultState({
            outcome: "CONFLICT",
            message: "权限已更新",
            serverPermissionVersion: "pv-live-9",
        })
        expect(result?.status).toBe("blocked")
        expect(result?.title).toBe("权限已更新")
        expect(result?.facts).toEqual([{ label: "当前版本", value: "pv-live-9" }])
    })

    it("maps an UNKNOWN outcome to an unknown result with the idempotency key", () => {
        const result = accessChangeResultState({
            outcome: "UNKNOWN",
            message: "处理结果待确认",
            idempotencyKey: "op_9",
        })
        expect(result?.status).toBe("unknown")
        expect(result?.pendingIdempotencyKey).toBe("op_9")
    })
})
