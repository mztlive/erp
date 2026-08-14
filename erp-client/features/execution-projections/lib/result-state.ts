/**
 * W23 执行信息 · 投递命令结果 → 页面反馈状态映射（纯函数，无 React）。
 */

import { type ResultState } from "@/components/business/feedback"
import { w29Href } from "@/features/execution-projections/lib/url-state"
import type { ProjectionDeliveryCommandResult } from "@/features/execution-projections/types"

export function commandToResultState(
    result: ProjectionDeliveryCommandResult,
): ResultState {
    if (result.stillUnknown || result.result === "STILL_UNKNOWN") {
        return {
            status: "unknown",
            title: "结果仍未知",
            description:
                "未明确前不显示成功、不跳过、不计入已确认指标。请再次查询或升级到接口错误中心。",
            reference: result.operationId,
            stayUnknown: true,
            facts: [
                { label: "操作编号", value: result.operationId },
                {
                    label: "对象",
                    value: `${result.salesOrderNo} · ${result.deliveryId}`,
                },
                { label: "时间", value: result.occurredAt },
                { label: "下一步", value: result.nextAction },
            ],
            w29Href: w29Href(result.workItemId, result.errorTaskId),
        }
    }
    if (result.result === "ESCALATED") {
        return {
            status: "succeeded",
            title: "已升级到错误中心",
            description:
                "处理任务仅在错误中心建立责任并完成；本页不提供任务处理。",
            reference: result.operationId,
            facts: [
                { label: "操作编号", value: result.operationId },
                {
                    label: "对象",
                    value: `${result.salesOrderNo} · ${result.deliveryId}`,
                },
                { label: "时间", value: result.occurredAt },
                { label: "下一步", value: result.nextAction },
                {
                    label: "错误中心任务",
                    value: result.workItemId ?? result.errorTaskId ?? "—",
                },
            ],
            w29Href: w29Href(result.workItemId, result.errorTaskId),
        }
    }
    if (result.result === "FAILED") {
        return {
            status: "blocked",
            title: result.resultLabel,
            description: "销售记录与应收未回退。可重试发送或转到接口错误中心。",
            reference: result.operationId,
            facts: [
                { label: "操作编号", value: result.operationId },
                {
                    label: "对象",
                    value: `${result.salesOrderNo} · ${result.deliveryId}`,
                },
                { label: "时间", value: result.occurredAt },
                { label: "下一步", value: result.nextAction },
            ],
        }
    }
    return {
        status: "succeeded",
        title: result.resultLabel,
        description: result.nextAction,
        reference: result.operationId,
        facts: [
            { label: "操作编号", value: result.operationId },
            {
                label: "对象",
                value: `${result.salesOrderNo} · ${result.deliveryId}`,
            },
            { label: "时间", value: result.occurredAt },
            { label: "下一步", value: result.nextAction },
        ],
    }
}
