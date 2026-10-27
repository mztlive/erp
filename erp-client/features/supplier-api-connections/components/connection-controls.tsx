"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
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

export function ConnectionControls({
    conn,
    open,
    onOpenChange,
}: {
    conn: ConnectionCenterView
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const enable = useEnableConnectionMutation()
    const disable = useDisableConnectionMutation()
    const test = useRunHealthCheckMutation()
    const [message, setMessage] = React.useState<string | null>(null)
    const [failed, setFailed] = React.useState(false)
    const pending = enable.isPending || disable.isPending || test.isPending
    const enabled = conn.status === "ENABLED"
    const action = enabled ? "DISABLE" : "ENABLE"
    const canChange = conn.allowedActions.includes(action)
    const blocker = conn.actionBlockers.find((item) => item.action === action)
    const run = async (operation: () => Promise<FormalOutcome>) => {
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
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="supplier-catalog-connection-controls-close"
                className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>{conn.supplier.name} · 连接设置</DialogTitle>
                    <DialogDescription>
                        {conn.deploymentMessage}
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-5">
                    <div className="space-y-2">
                        <p className="text-sm font-medium">
                            自动下单{enabled ? "已开启" : "已关闭"}
                        </p>
                        <p className="text-sm text-muted-foreground">
                            开启时自动检查连接，现有采购审批与执行条件继续生效。关闭后停止新增发单，商品同步、选品与已有订单跟踪继续运行。
                        </p>
                        <LoadingButton
                            id="supplier-catalog-auto-order-toggle"
                            loading={enable.isPending || disable.isPending}
                            disabled={!canChange || pending}
                            onClick={() =>
                                void run(() =>
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
                        {!canChange ? (
                            <p className="text-sm text-muted-foreground">
                                {blocker?.message ??
                                    "当前账号没有修改自动下单设置的权限。"}
                            </p>
                        ) : null}
                    </div>
                    <div className="space-y-2">
                        <p className="text-sm font-medium">接口支持的操作</p>
                        <p className="text-sm text-muted-foreground">
                            {conn.declaredCapabilities
                                .map((code) => CAPABILITY_LABEL[code])
                                .join("、") || "接口未配置，暂时无法读取能力。"}
                        </p>
                    </div>
                    <Button
                        id="supplier-api-connections-center-run-health"
                        variant="outline"
                        disabled={
                            !conn.allowedActions.includes("RUN_HEALTH_CHECK") ||
                            pending
                        }
                        onClick={() =>
                            void run(() =>
                                test.mutateAsync({
                                    connectionId: conn.connectionId,
                                    expectedVersion: conn.version,
                                    checkType: "AUTHENTICATION",
                                    idempotencyKey: newIdempotencyKey("health"),
                                }),
                            )
                        }
                    >
                        {test.isPending ? "正在提交…" : "测试连接"}
                    </Button>
                    {message ? (
                        <p
                            role={failed ? "alert" : "status"}
                            className={
                                failed ? "text-sm text-destructive" : "text-sm"
                            }
                        >
                            {message}
                        </p>
                    ) : null}
                    <HealthRecords records={conn.healthRecords} />
                </div>
            </DialogContent>
        </Dialog>
    )
}
