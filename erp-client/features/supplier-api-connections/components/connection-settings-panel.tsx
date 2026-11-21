"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import * as React from "react"
import { PlugZapIcon, ShoppingCartIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { JdVopAuthorizationSection } from "./sections/jd-vop-authorization"

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
    const canCreateOrders =
        conn.deploymentReady && conn.declaredCapabilities.includes("ORDER")
    const canChange =
        conn.allowedActions.includes(action) && (enabled || canCreateOrders)
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
                    ? "text-body-compact text-destructive"
                    : "text-xs text-muted-foreground"
            }
        >
            {message}
        </p>
    ) : null
    return (
        <div className="space-y-4">
            {!conn.deploymentReady ? (
                <Alert variant="warning">
                    <AlertTitle>供应商接口尚未就绪</AlertTitle>
                    <AlertDescription>
                        {conn.deploymentMessage ||
                            "请联系管理员完成服务端接口配置并核验授权。"}
                    </AlertDescription>
                </Alert>
            ) : !canCreateOrders ? (
                <Alert variant="info">
                    <AlertTitle>当前尚未开放采购下单</AlertTitle>
                    <AlertDescription>
                        {conn.deploymentMessage ||
                            "可用功能按实际接口能力显示，完成采购执行条件与上线核验后才能开放新的订单。"}
                    </AlertDescription>
                </Alert>
            ) : null}
            {conn.alerts.map((alert) => (
                <Alert key={alert.id} variant={alert.severity}>
                    <AlertTitle>{alert.title}</AlertTitle>
                    <AlertDescription>{alert.description}</AlertDescription>
                </Alert>
            ))}
            <div className="min-w-0 divide-y rounded-xl border bg-card">
                <section aria-label="自动下单" className="space-y-4 p-6">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0 space-y-2">
                            <h2 className="flex items-center gap-2 text-section-title font-semibold tracking-tight">
                                <ShoppingCartIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                自动下单
                            </h2>
                            <p className="text-xs text-muted-foreground">
                                {enabled
                                    ? "允许向供应商发送新的采购订单。"
                                    : "已暂停向供应商发送新的采购订单。"}
                            </p>
                        </div>
                        <PermissionGuard
                            permission={[
                                "supplier_api_connection:command",
                                `supplier_api_connection:${enabled ? "disable" : "enable"}`,
                            ]}
                        >
                            <LoadingButton
                                id={`supplier-api-connection-${segment}-auto-order-toggle`}
                                loading={enable.isPending || disable.isPending}
                                disabled={!canChange || pending}
                                onClick={() =>
                                    void run("order", () =>
                                        enabled
                                            ? disable.mutateAsync({
                                                  connectionId:
                                                      conn.connectionId,
                                                  expectedVersion: conn.version,
                                                  reasonCode:
                                                      "PAUSE_NEW_ORDERS",
                                                  idempotencyKey:
                                                      newIdempotencyKey(
                                                          "disable",
                                                      ),
                                              })
                                            : enable.mutateAsync({
                                                  connectionId:
                                                      conn.connectionId,
                                                  expectedVersion: conn.version,
                                                  idempotencyKey:
                                                      newIdempotencyKey(
                                                          "enable",
                                                      ),
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
                        </PermissionGuard>
                    </div>
                    <p className="text-body-compact leading-6 text-muted-foreground">
                        开启前自动验证连接，采购审批与执行条件仍须满足。
                        关闭不影响商品同步、选品、原请求查询、取消处理和已有订单跟踪。
                    </p>
                    {messageTarget === "order" ? feedback : null}
                    {!canChange ? (
                        <PermissionGuard
                            permission={[
                                "supplier_api_connection:command",
                                `supplier_api_connection:${enabled ? "disable" : "enable"}`,
                            ]}
                        >
                            <p className="text-xs text-muted-foreground">
                                {blocker?.message ??
                                    (!canCreateOrders && !enabled
                                        ? "当前接口未具备采购下单能力，请联系管理员核验配置和接入条件。"
                                        : "当前账号没有修改自动下单设置的权限。")}
                            </p>
                        </PermissionGuard>
                    ) : null}
                </section>
                {conn.connectorType?.toLowerCase() === "jd_vop" ? (
                    <JdVopAuthorizationSection
                        connectionId={conn.connectionId}
                        disabled={pending}
                    />
                ) : null}
                <section aria-label="连接测试" className="space-y-4 p-6">
                    <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="min-w-0 space-y-2">
                            <h2 className="flex items-center gap-2 text-section-title font-semibold tracking-tight">
                                <PlugZapIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                连接测试
                            </h2>
                            <p className="text-xs text-muted-foreground">
                                验证供应商接口访问权限，不会创建订单。
                            </p>
                        </div>
                        <PermissionGuard
                            permission={[
                                "supplier_api_connection:command",
                                "supplier_api_connection:health_check",
                            ]}
                        >
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
                        </PermissionGuard>
                    </div>
                    {messageTarget === "test" ? feedback : null}
                    {!conn.allowedActions.includes("RUN_HEALTH_CHECK") ? (
                        <PermissionGuard
                            permission={[
                                "supplier_api_connection:command",
                                "supplier_api_connection:health_check",
                            ]}
                        >
                            <p className="text-xs text-muted-foreground">
                                {testBlocker?.message ??
                                    "当前账号没有测试连接的权限。"}
                            </p>
                        </PermissionGuard>
                    ) : null}
                    <HealthRecords
                        records={conn.healthRecords}
                        idPrefix={`supplier-api-connection-${segment}-health`}
                    />
                </section>
                <section aria-label="接口信息" className="space-y-4 p-6">
                    <div className="space-y-2">
                        <h2 className="text-section-title font-semibold tracking-tight">
                            {conn.connectorTypeLabel}
                        </h2>
                        {conn.deploymentMessage && canCreateOrders ? (
                            <p className="text-body-compact leading-6 text-muted-foreground">
                                {conn.deploymentMessage}
                            </p>
                        ) : null}
                    </div>
                    <div className="space-y-3">
                        <h3 className="font-medium">支持的功能</h3>
                        <div className="flex flex-wrap gap-2">
                            {conn.declaredCapabilities.map((code) => (
                                <Badge key={code} variant="outline">
                                    {CAPABILITY_LABEL[code]}
                                </Badge>
                            ))}
                        </div>
                        {conn.declaredCapabilities.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                当前没有已实现并通过配置校验的接口能力，请先核验配置与账号授权。
                            </p>
                        ) : null}
                    </div>
                </section>
            </div>
        </div>
    )
}
