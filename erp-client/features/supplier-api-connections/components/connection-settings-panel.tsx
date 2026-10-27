"use client"

import * as React from "react"
import { PlugZapIcon, ShoppingCartIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useDisableConnectionMutation,
    useEnableConnectionMutation,
    useRunHealthCheckMutation,
} from "../hooks/queries"
import {
    CAPABILITY_LABEL,
    type ConnectionCenterView,
    type FormalOutcome,
} from "../types"
import { newIdempotencyKey } from "../lib/operations"
import { HealthRecords } from "./sections/health-records"

/** 一个供应商连接的 API 设置：自动下单、接口能力与连接测试。 */
export function ConnectionSettingsPanel({
    conn,
}: {
    conn: ConnectionCenterView
}) {
    const enable = useEnableConnectionMutation()
    const disable = useDisableConnectionMutation()
    const test = useRunHealthCheckMutation()
    const [message, setMessage] = React.useState<string | null>(null)
    const [failed, setFailed] = React.useState(false)
    const [messageTarget, setMessageTarget] = React.useState<"order" | "test">(
        "order",
    )
    const pending = enable.isPending || disable.isPending || test.isPending
    const enabled = conn.status === "ENABLED"
    const action = enabled ? "DISABLE" : "ENABLE"
    const canChange = conn.allowedActions.includes(action)
    const blocker = conn.actionBlockers.find((item) => item.action === action)
    const segment = toAutomationIdSegment(conn.connectionId)
    const testBlocker = conn.actionBlockers.find(
        (item) => item.action === "RUN_HEALTH_CHECK",
    )
    const run = async (
        target: "order" | "test",
        operation: () => Promise<FormalOutcome>,
    ) => {
        setMessageTarget(target)
        setMessage(null)
        setFailed(false)
        try {
            const result = await operation()
            setMessage(result.message)
            setFailed(
                result.status === "failed" || result.status === "rejected",
            )
        } catch (error) {
            setFailed(true)
            setMessage(
                error instanceof Error ? error.message : "操作失败，请重试。",
            )
        }
    }
    const feedback = message ? (
        <p
            role={failed ? "alert" : "status"}
            className={
                failed
                    ? "text-sm text-destructive"
                    : "text-sm text-muted-foreground"
            }
        >
            {message}
        </p>
    ) : null
    return (
        <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_20rem]">
            <div className="min-w-0 divide-y rounded-xl border bg-card">
                <section aria-label="自动下单" className="space-y-4 p-6">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0 space-y-2">
                            <h2 className="flex items-center gap-2 text-base font-semibold">
                                <ShoppingCartIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                自动下单
                            </h2>
                            <p className="text-sm text-muted-foreground">
                                {enabled
                                    ? "允许向供应商发送新的采购订单。"
                                    : "已暂停向供应商发送新的采购订单。"}
                            </p>
                        </div>
                        <LoadingButton
                            id={`supplier-api-connection-${segment}-auto-order-toggle`}
                            loading={enable.isPending || disable.isPending}
                            disabled={!canChange || pending}
                            onClick={() =>
                                void run("order", () =>
                                    enabled
                                        ? disable.mutateAsync({
                                              connectionId: conn.connectionId,
                                              expectedVersion: conn.version,
                                              reasonCode: "PAUSE_NEW_ORDERS",
                                              idempotencyKey:
                                                  newIdempotencyKey("disable"),
                                          })
                                        : enable.mutateAsync({
                                              connectionId: conn.connectionId,
                                              expectedVersion: conn.version,
                                              idempotencyKey:
                                                  newIdempotencyKey("enable"),
                                          }),
                                )
                            }
                        >
                            {enable.isPending
                                ? "正在检查连接并开启…"
                                : enabled
                                  ? "关闭自动下单"
                                  : "允许自动下单"}
                        </LoadingButton>
                    </div>
                    <p className="text-sm leading-6 text-muted-foreground">
                        开启前自动验证连接，采购审批与执行条件仍须满足。
                        关闭不影响商品同步、选品和已有订单跟踪。
                    </p>
                    {messageTarget === "order" ? feedback : null}
                    {!canChange ? (
                        <p className="text-sm text-muted-foreground">
                            {blocker?.message ??
                                "当前账号没有修改自动下单设置的权限。"}
                        </p>
                    ) : null}
                </section>
                <section aria-label="连接测试" className="space-y-4 p-6">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0 space-y-2">
                            <h2 className="flex items-center gap-2 text-base font-semibold">
                                <PlugZapIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                连接测试
                            </h2>
                            <p className="text-sm text-muted-foreground">
                                验证供应商接口访问权限，不会创建订单。
                            </p>
                        </div>
                        <LoadingButton
                            id={`supplier-api-connection-${segment}-run-health`}
                            variant="outline"
                            loading={test.isPending}
                            disabled={
                                !conn.allowedActions.includes(
                                    "RUN_HEALTH_CHECK",
                                ) || pending
                            }
                            onClick={() =>
                                void run("test", () =>
                                    test.mutateAsync({
                                        connectionId: conn.connectionId,
                                        expectedVersion: conn.version,
                                        checkType: "AUTHENTICATION",
                                        idempotencyKey:
                                            newIdempotencyKey("health"),
                                    }),
                                )
                            }
                        >
                            {test.isPending ? "正在提交…" : "测试连接"}
                        </LoadingButton>
                    </div>
                    {messageTarget === "test" ? feedback : null}
                    {!conn.allowedActions.includes("RUN_HEALTH_CHECK") ? (
                        <p className="text-sm text-muted-foreground">
                            {testBlocker?.message ??
                                "当前账号没有测试连接的权限。"}
                        </p>
                    ) : null}
                    <HealthRecords
                        records={conn.healthRecords}
                        idPrefix={`supplier-api-connection-${segment}-health`}
                    />
                </section>
            </div>
            <aside
                aria-label="接口信息"
                className="min-w-0 space-y-5 rounded-xl bg-muted/40 p-6"
            >
                <div className="space-y-2">
                    <h2 className="text-sm font-semibold">
                        {conn.connectorTypeLabel}
                    </h2>
                    {conn.deploymentMessage ? (
                        <p className="text-sm leading-6 text-muted-foreground">
                            {conn.deploymentMessage}
                        </p>
                    ) : null}
                </div>
                <div className="space-y-3 border-t pt-5">
                    <h3 className="text-sm font-medium">支持的功能</h3>
                    <div className="flex flex-wrap gap-2">
                        {conn.declaredCapabilities.map((code) => (
                            <Badge key={code} variant="outline">
                                {CAPABILITY_LABEL[code]}
                            </Badge>
                        ))}
                    </div>
                    {conn.declaredCapabilities.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            接口未配置，暂时无法读取能力。
                        </p>
                    ) : null}
                </div>
            </aside>
        </div>
    )
}
