"use client"

import { ShieldAlertIcon, TriangleAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"

/** 连接中心告警区：服务端告警、鉴权失败与结果待确认。 */
export function CenterAlerts({ conn }: { conn: ConnectionCenterView }) {
    const authFailed = conn.lastHealth?.result === "AUTH_FAILED"
    const resultUnknown = conn.lastHealth?.result === "UNKNOWN"
    return (
        <>
            {!conn.connectorType ? (
                <Alert variant="warning">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>请补充接口类型</AlertTitle>
                    <AlertDescription>
                        此连接尚未指定接口类型，请联系管理员补充后再绑定配置或执行检查。
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
