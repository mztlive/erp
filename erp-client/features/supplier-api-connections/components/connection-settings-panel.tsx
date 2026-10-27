"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
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
    const pending = enable.isPending || disable.isPending || test.isPending
    const enabled = conn.status === "ENABLED"
    const action = enabled ? "DISABLE" : "ENABLE"
    const canChange = conn.allowedActions.includes(action)
    const blocker = conn.actionBlockers.find((item) => item.action === action)
    const segment = toAutomationIdSegment(conn.connectionId)
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
        <div className="max-w-3xl space-y-5">
            <section
                aria-label="自动下单"
                className="space-y-3 rounded-xl border bg-card p-5"
            >
                <div className="space-y-2">
                    <p className="text-sm font-medium">
                        自动下单{enabled ? "已开启" : "已关闭"}
                    </p>
                    <p className="text-sm text-muted-foreground">
                        开启时自动检查连接，现有采购审批与执行条件继续生效。关闭后停止新增发单，商品同步、选品与已有订单跟踪继续运行。
                    </p>
                </div>
                <LoadingButton
                    id={`supplier-api-connection-${segment}-auto-order-toggle`}
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
            </section>
            <section
                aria-label="接口支持的操作"
                className="space-y-2 rounded-xl border bg-card p-5"
            >
                <p className="text-sm font-medium">接口支持的操作</p>
                <p className="text-sm text-muted-foreground">
                    {conn.declaredCapabilities
                        .map((code) => CAPABILITY_LABEL[code])
                        .join("、") || "接口未配置，暂时无法读取能力。"}
                </p>
            </section>
            <section
                aria-label="连接测试"
                className="space-y-3 rounded-xl border bg-card p-5"
            >
                <div className="space-y-2">
                    <p className="text-sm font-medium">连接测试</p>
                    <p className="text-sm text-muted-foreground">
                        检查当前配置的只读鉴权是否可用。
                    </p>
                </div>
                <Button
                    id={`supplier-api-connection-${segment}-run-health`}
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
            </section>
            <HealthRecords
                records={conn.healthRecords}
                idPrefix={`supplier-api-connection-${segment}-health`}
            />
        </div>
    )
}
