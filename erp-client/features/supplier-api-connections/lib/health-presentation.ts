import type {
    ConnectionCenterView,
    ConnectionSection,
    HealthResult,
} from "@/features/supplier-api-connections/types"

export const HEALTH_PRESENTATION: Record<
    HealthResult,
    { title: string; description: string }
> = {
    SUCCESS: {
        title: "最近一次连接检查通过",
        description: "连接检查已完成，能力验证状态请以各项检查结果为准。",
    },
    FAILED: {
        title: "连接检查未通过",
        description: "请根据检查记录核对接口地址、访问凭据与供应商服务状态。",
    },
    PARTIAL: {
        title: "部分能力需要检查",
        description: "部分能力未通过验证，请查看检查结果并处理相关配置。",
    },
    UNCHECKED: {
        title: "尚未执行健康检查",
        description: "完成连接配置后执行健康检查，确认当前接口是否可用。",
    },
    STALE: {
        title: "检查结果需要更新",
        description: "已有检查结果已过期，请重新检查当前连接。",
    },
    AUTH_FAILED: {
        title: "访问凭据验证未通过",
        description: "请核对访问凭据与供应商接口权限；自动重试已停止。",
    },
    UNKNOWN: {
        title: "检查结果待确认",
        description:
            "检查仍在进行或结果尚未确认，请刷新检查结果，避免重复提交。",
    },
}

type HealthAdvice = {
    title: string
    description: string
    section?: ConnectionSection
    actionLabel?: string
}

export function connectionHealthAdvice(
    conn: ConnectionCenterView,
): HealthAdvice {
    if (conn.lastHealth?.result === "UNKNOWN") {
        return {
            title: "等待检查结果",
            description:
                "当前检查尚无明确结论。请刷新检查结果，并查看下方记录中的处理状态。",
        }
    }
    const { endpoint, credential } = conn.safeReferences
    if (endpoint.state === "MISSING" || credential.state === "MISSING") {
        return {
            title: "完成连接配置",
            description:
                "接口地址和访问凭据均须完成绑定。请前往安全配置，核对尚未完成的项目。",
            section: "security",
            actionLabel: "查看安全配置",
        }
    }
    if (
        conn.lastHealth?.result === "AUTH_FAILED" ||
        credential.state === "ROTATION_DUE"
    ) {
        return {
            title: "核对访问凭据及接口权限",
            description:
                "请确认凭据有效且供应商已开通所需权限。修改配置后重新执行健康检查。",
            section: "security",
            actionLabel: "查看安全配置",
        }
    }
    if (conn.lastHealth?.result === "FAILED") {
        return {
            title: "核对连接配置与检查记录",
            description:
                "请结合下方检查记录排查接口地址、访问凭据和供应商服务状态，再重新检查。",
            section: "security",
            actionLabel: "查看安全配置",
        }
    }
    if (
        !conn.capabilities.length ||
        conn.capabilities.some(
            (item) =>
                item.status === "ENABLED" && item.verification !== "SUCCESS",
        )
    ) {
        return {
            title: "确认能力配置与验证状态",
            description:
                "请核对业务需要的能力及其验证状态。连接检查通过不代表每项能力或每件商品均可使用。",
            section: "capabilities",
            actionLabel: "查看能力配置",
        }
    }
    return {
        title: "按需检查当前连接",
        description: conn.nextStep,
        section: "overview",
        actionLabel: "查看连接概览",
    }
}
