import * as React from "react"

import type {
    ApproveConclusion,
    FormalOutcome,
} from "@/features/card-funds-review/types"
import { APPROVE_CONCLUSION_LABEL } from "@/features/card-funds-review/types"

/** 复核结果条的事实列表（纯函数，随结果条组件渲染）。 */
export function buildResultFacts(
    outcome?: FormalOutcome,
): { label: string; value: React.ReactNode }[] {
    if (!outcome) return []
    const biz = outcome.business
    const facts = [
        { label: "复核号", value: String(biz.reviewNo) },
        {
            label: "结论",
            value:
                biz.conclusion === "REJECTED"
                    ? "驳回"
                    : APPROVE_CONCLUSION_LABEL[
                          biz.conclusion as ApproveConclusion
                      ],
        },
        {
            label: "完成时间",
            value: new Date(biz.completedAt).toLocaleString("zh-CN", {
                hour12: false,
            }),
        },
        { label: "操作号", value: biz.operationId },
    ]
    return facts
}
