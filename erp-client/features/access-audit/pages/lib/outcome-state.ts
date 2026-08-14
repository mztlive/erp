import type { ResultState } from "@/components/business/feedback"
import { formatDateTime } from "@/lib/datetime"
import { resultText } from "@/lib/ui-text"
import type { AccessChangeOutcome } from "@/features/access-audit/types"

/** 把提交结果映射成页面结果条状态（纯函数，便于测试）。 */
export function accessChangeResultState(
    outcome: AccessChangeOutcome,
): ResultState {
    if (outcome.outcome === "CONFIRMED") {
        return {
            status: "succeeded",
            title: "授权变更已生效",
            description: outcome.message,
            reference: outcome.reference,
            facts: [
                {
                    label: "配置版本",
                    value: `v${outcome.permissionVersion.split("-").at(-1)}`,
                },
                {
                    label: "影响主体数",
                    value: String(outcome.affectedSubjectCount),
                },
                { label: "审计事件号", value: outcome.auditEventId },
                {
                    label: "生效时间",
                    value: formatDateTime(outcome.effectiveAt, "full"),
                },
                {
                    label: "下一步",
                    value: outcome.nextSteps.join("；"),
                },
            ],
        }
    }
    if (outcome.outcome === "REJECTED") {
        return {
            status:
                outcome.code === "REVIEW_POLICY_UNCONFIGURED"
                    ? "blocked"
                    : "rejected",
            title:
                outcome.code === "REVIEW_POLICY_UNCONFIGURED"
                    ? "复核策略未确定，动作已阻断"
                    : "授权变更被拒绝",
            description: outcome.message,
            facts: outcome.actionBlockers?.map((b) => ({
                label: b.code,
                value: b.message,
            })),
        }
    }
    if (outcome.outcome === "CONFLICT") {
        return {
            status: "blocked",
            title: "权限已更新",
            description: outcome.message,
            facts: [
                {
                    label: "当前版本",
                    value: outcome.serverPermissionVersion,
                },
            ],
        }
    }
    return {
        status: "unknown",
        title: resultText.unknown,
        description: outcome.message,
        pendingIdempotencyKey: outcome.idempotencyKey,
    }
}
