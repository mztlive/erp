"use client"

import * as React from "react"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import {
    BusinessStatusBadge,
    DetailWorkspaceBody,
    DetailWorkspaceSection,
    FormalActionConfirmDialog,
    OptionCombobox,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Label } from "@/components/ui/label"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
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

type SectionFeedback = {
    tone: "info" | "success" | "warning" | "destructive"
    title: string
    message: string
}

const DISABLE_REASONS = [
    { value: "PAUSE_NEW_ORDERS", label: "暂停向供应商发送新订单" },
    { value: "MAINTENANCE", label: "供应商平台维护或接口升级" },
    { value: "COST_CHECK", label: "核对价格、库存或对账异常" },
    { value: "AUTH_ROTATION", label: "轮换账号授权或密钥" },
] as const

function outcomeFeedback(
    titlePrefix: string,
    result: FormalOutcome,
): SectionFeedback {
    if (result.status === "failed" || result.status === "rejected") {
        return {
            tone: "destructive",
            title: `${titlePrefix}未完成`,
            message: result.message,
        }
    }
    if (result.status === "unknown" || result.status === "blocked") {
        return {
            tone: "warning",
            title: `${titlePrefix}待确认`,
            message: result.message,
        }
    }
    return {
        tone: result.status === "processing" ? "info" : "success",
        title:
            result.status === "processing"
                ? `${titlePrefix}已提交`
                : `${titlePrefix}成功`,
        message: result.message,
    }
}

/** 一个供应商连接的 API 设置：自动下单、接口能力与连接测试。 */
export function ConnectionSettingsPanel({
    conn,
}: {
    conn: ConnectionCenterView
}) {
    const enable = useEnableConnectionMutation()
    const disable = useDisableConnectionMutation()
    const test = useRunHealthCheckMutation()
    const [orderFeedback, setOrderFeedback] =
        React.useState<SectionFeedback | null>(null)
    const [testFeedback, setTestFeedback] =
        React.useState<SectionFeedback | null>(null)
    const [toggleConfirmOpen, setToggleConfirmOpen] = React.useState(false)
    const [disableReason, setDisableReason] =
        React.useState<string>("PAUSE_NEW_ORDERS")

    const orderPending = enable.isPending || disable.isPending
    const testPending = test.isPending
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

    const handleConfirmToggle = async () => {
        setOrderFeedback(null)
        try {
            const result = enabled
                ? await disable.mutateAsync({
                      connectionId: conn.connectionId,
                      expectedVersion: conn.version,
                      reasonCode: disableReason || "PAUSE_NEW_ORDERS",
                      idempotencyKey: newIdempotencyKey("disable"),
                  })
                : await enable.mutateAsync({
                      connectionId: conn.connectionId,
                      expectedVersion: conn.version,
                      idempotencyKey: newIdempotencyKey("enable"),
                  })
            const nextFeedback = outcomeFeedback(
                enabled ? "关闭自动下单" : "允许自动下单",
                result,
            )
            setOrderFeedback(nextFeedback)
            if (nextFeedback.tone === "success") {
                toast.add({
                    type: "success",
                    title: nextFeedback.title,
                    description: nextFeedback.message,
                })
            } else if (nextFeedback.tone === "destructive") {
                throw new Error(nextFeedback.message)
            }
        } catch (error) {
            const msg = getErrorMessage(error, "操作失败，请稍后重试。")
            setOrderFeedback({
                tone: "destructive",
                title: enabled ? "关闭自动下单失败" : "允许自动下单失败",
                message: msg,
            })
            throw error
        }
    }

    const handleRunTest = async () => {
        setTestFeedback(null)
        try {
            const result = await test.mutateAsync({
                connectionId: conn.connectionId,
                expectedVersion: conn.version,
                checkType: "AUTHENTICATION",
                idempotencyKey: newIdempotencyKey("health"),
            })
            const nextFeedback = outcomeFeedback("连接测试", result)
            setTestFeedback(nextFeedback)
            if (
                nextFeedback.tone === "success" ||
                nextFeedback.tone === "info"
            ) {
                toast.add({
                    type: nextFeedback.tone,
                    title: nextFeedback.title,
                    description: nextFeedback.message,
                })
            }
        } catch (error) {
            const msg = getErrorMessage(error, "连接测试提交失败，请重试。")
            setTestFeedback({
                tone: "destructive",
                title: "连接测试失败",
                message: msg,
            })
        }
    }

    return (
        <DetailWorkspaceBody
            alerts={
                <>
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
                            <AlertDescription>
                                {alert.description}
                            </AlertDescription>
                        </Alert>
                    ))}
                </>
            }
        >
            <DetailWorkspaceSection
                title="自动下单"
                ariaLabel="自动下单"
                width="form"
                actions={
                    <PermissionGuard
                        permission={[
                            "supplier_api_connection:command",
                            `supplier_api_connection:${enabled ? "disable" : "enable"}`,
                        ]}
                    >
                        <LoadingButton
                            id={`supplier-api-connection-${segment}-auto-order-toggle`}
                            size="sm"
                            variant={enabled ? "ghost" : "default"}
                            className={
                                enabled
                                    ? detailWorkspaceStyles.quietDangerButton
                                    : undefined
                            }
                            loading={orderPending}
                            disabled={!canChange || orderPending}
                            onClick={() => {
                                setDisableReason("PAUSE_NEW_ORDERS")
                                setToggleConfirmOpen(true)
                            }}
                        >
                            {enable.isPending
                                ? "正在检查连接并开启…"
                                : disable.isPending
                                  ? "正在关闭…"
                                  : enabled
                                    ? "关闭自动下单"
                                    : "允许自动下单"}
                        </LoadingButton>
                    </PermissionGuard>
                }
            >
                <p className="text-xs text-muted-foreground">
                    {enabled
                        ? "允许向供应商发送新的采购订单。"
                        : "已暂停向供应商发送新的采购订单。"}
                </p>
                <p className="text-body-compact text-muted-foreground">
                    开启前自动验证连接，采购审批与执行条件仍须满足。
                    关闭不影响商品同步、选品、原请求查询、取消处理和已有订单跟踪。
                </p>
                {orderFeedback ? (
                    <Alert variant={orderFeedback.tone}>
                        <AlertTitle>{orderFeedback.title}</AlertTitle>
                        <AlertDescription>
                            {orderFeedback.message}
                        </AlertDescription>
                    </Alert>
                ) : null}
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
            </DetailWorkspaceSection>

            {conn.connectorType?.toLowerCase() === "jd_vop" ? (
                <JdVopAuthorizationSection
                    connectionId={conn.connectionId}
                    disabled={orderPending}
                />
            ) : null}

            <DetailWorkspaceSection
                title="连接测试"
                ariaLabel="连接测试"
                width="form"
                actions={
                    <PermissionGuard
                        permission={[
                            "supplier_api_connection:command",
                            "supplier_api_connection:health_check",
                        ]}
                    >
                        <LoadingButton
                            id={`supplier-api-connection-${segment}-run-health`}
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            loading={testPending}
                            disabled={
                                !conn.allowedActions.includes(
                                    "RUN_HEALTH_CHECK",
                                ) || testPending
                            }
                            onClick={() => void handleRunTest()}
                        >
                            {testPending ? "正在提交…" : "测试连接"}
                        </LoadingButton>
                    </PermissionGuard>
                }
            >
                <p className="text-xs text-muted-foreground">
                    验证供应商接口访问权限，不会创建订单。
                </p>
                {testFeedback ? (
                    <Alert variant={testFeedback.tone}>
                        <AlertTitle>{testFeedback.title}</AlertTitle>
                        <AlertDescription>
                            {testFeedback.message}
                        </AlertDescription>
                    </Alert>
                ) : null}
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
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                ariaLabel="接口信息"
                title={conn.connectorTypeLabel}
                width="form"
            >
                {conn.deploymentMessage && canCreateOrders ? (
                    <p className="text-body-compact text-muted-foreground">
                        {conn.deploymentMessage}
                    </p>
                ) : null}
                <div className="space-y-3">
                    <h3 className="font-medium">支持的功能</h3>
                    <div className="flex flex-wrap gap-2">
                        {conn.declaredCapabilities.map((code) => (
                            <BusinessStatusBadge
                                key={code}
                                context="list"
                                appearance="quiet"
                                tone="neutral"
                                label={CAPABILITY_LABEL[code]}
                            />
                        ))}
                    </div>
                    {conn.declaredCapabilities.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            当前没有已实现并通过配置校验的接口能力，请先核验配置与账号授权。
                        </p>
                    ) : null}
                </div>
            </DetailWorkspaceSection>

            <FormalActionConfirmDialog
                idPrefix={`supplier-api-connection-${segment}-auto-order-confirm`}
                open={toggleConfirmOpen}
                onOpenChange={(next) => {
                    if (!orderPending) setToggleConfirmOpen(next)
                }}
                pending={orderPending}
                actionVariant={enabled ? "destructive" : "default"}
                title={enabled ? "关闭自动下单" : "允许自动下单"}
                description={
                    enabled
                        ? "关闭后系统将暂停向该供应商平台自动提交新采购订单，请确认并选择暂停原因。"
                        : "开启前系统会自动验证接口连接；开启后符合审批和预算条件的采购单将自动提交至该供应商平台。"
                }
                actionLabel={enabled ? "关闭自动下单" : "允许自动下单"}
                confirmLabel={enabled ? "确认关闭自动下单" : "确认允许自动下单"}
                fromStatus={{
                    label: conn.statusLabel,
                    tone: conn.statusTone,
                }}
                toStatus={
                    enabled
                        ? { label: "已暂停自动下单", tone: "neutral" }
                        : { label: "已启用自动下单", tone: "success" }
                }
                summary={[
                    <span key="supplier">供应商：{conn.supplier.name}</span>,
                    <span key="code">
                        连接代码：
                        <span className="num font-medium text-foreground">
                            {conn.connectionCode}
                        </span>
                    </span>,
                ]}
                effects={
                    enabled
                        ? [
                              "停止向该供应商提交新的自动采购订单。",
                              "不影响商品库同步、选品、原请求查询、取消处理及已有订单跟踪。",
                          ]
                        : [
                              "系统将先验证接口连接健康状态。",
                              "仅对已完成采购审批且满足预算条件的订单生效。",
                          ]
                }
                formContent={
                    enabled ? (
                        <div className="space-y-1.5">
                            <Label
                                htmlFor={`supplier-api-connection-${segment}-disable-reason`}
                            >
                                暂停原因
                                <RequiredMark />
                            </Label>
                            <OptionCombobox
                                id={`supplier-api-connection-${segment}-disable-reason`}
                                value={disableReason}
                                onValueChange={(next) => {
                                    if (next) setDisableReason(next)
                                }}
                                options={DISABLE_REASONS.map((item) => ({
                                    value: item.value,
                                    label: item.label,
                                }))}
                                allowClear={false}
                                disabled={orderPending}
                            />
                        </div>
                    ) : undefined
                }
                confirmDisabled={enabled && !disableReason}
                onConfirm={handleConfirmToggle}
            />
        </DetailWorkspaceBody>
    )
}
