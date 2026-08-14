"use client"

import { ShieldAlertIcon, TriangleAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"

/** 连接中心告警区：生产提示、服务端告警、鉴权失败与结果待确认。 */
export function CenterAlerts({ conn }: { conn: ConnectionCenterView }) {
    const isProd = conn.environment === "PRODUCTION"
    const authFailed = conn.lastHealth?.result === "AUTH_FAILED"
    const resultUnknown = conn.lastHealth?.result === "UNKNOWN"
    return (
        <>
            {isProd ? (
                <Alert variant="warning" role="status">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>生产环境</AlertTitle>
                    <AlertDescription>
                        当前连接运行在生产环境。启停、密钥轮换与全能力检查均需二次确认；检查不会创建真实业务订单。
                    </AlertDescription>
                </Alert>
            ) : null}

            {conn.alerts.map((al) => (
                <Alert
                    key={al.id}
                    variant={
                        al.severity === "destructive"
                            ? "destructive"
                            : al.severity === "warning"
                              ? "warning"
                              : "default"
                    }
                    role="alert"
                >
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>{al.title}</AlertTitle>
                    <AlertDescription>{al.description}</AlertDescription>
                </Alert>
            ))}

            {authFailed &&
            !conn.alerts.some((a) => a.title.includes("鉴权")) ? (
                <Alert variant="destructive" role="alert">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>鉴权/签名失败 · 自动重试已停止</AlertTitle>
                    <AlertDescription>
                        {conn.lastHealth?.errorSummary ??
                            "高风险故障。请运维检查密钥引用与适配器；本页不展示密钥正文。"}
                    </AlertDescription>
                </Alert>
            ) : null}

            {resultUnknown ? (
                <Alert variant="warning" role="status" aria-live="polite">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>处理结果待确认</AlertTitle>
                    <AlertDescription>
                        不得按成功或失败处理，不乐观改变启停或引用状态。请按原任务号查询最终结论。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
