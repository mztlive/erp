import type { IntegrationResolutionItemView } from "../types"

export const INTEGRATION_ACTION_LABEL: Readonly<Record<string, string>> = {
    QUERY_ORIGINAL_RESULT: "查询原结果",
    REPLAY_ORIGINAL: "重新提交",
    ADD_EVIDENCE: "补充证据",
    LINK_COMPENSATION: "关联补偿",
    REATTRIBUTE: "重新归集",
    RESOLVE: "处理完成",
    CLOSE_DUPLICATE: "关闭重复",
    CLOSE_MISROUTED: "关闭错误路由",
    CONFIRM_NO_ERROR: "确认无误",
    CONFIRM_VALID_DIFFERENCE: "确认有效差异",
}

export function integrationStatusTone(
    item: IntegrationResolutionItemView,
): "destructive" | "warning" | "info" | "neutral" | "success" {
    const code = item.status.code
    if (
        code === "COMPLETED" ||
        code === "RESOLVED" ||
        code === "CONFIRMED_NO_ERROR" ||
        code === "CONFIRMED_VALID_DIFFERENCE"
    ) {
        return "success"
    }
    if (code === "CLOSED") {
        return "neutral"
    }
    if (
        code === "MANUAL_REQUIRED" ||
        code === "SECURITY_FAULT" ||
        item.status.label.includes("人工")
    ) {
        return "destructive"
    }
    if (code === "AUTO_RETRYING") return "info"
    return "warning"
}
