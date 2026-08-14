"use client"

import { KeyRoundIcon } from "lucide-react"

import { surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { RefLabel } from "@/features/supplier-api-connections/components/reference-label"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { cn } from "@/lib/utils"

export function SecuritySection({
    conn,
    onBind,
    onBindEndpoint,
}: {
    conn: ConnectionCenterView
    onBind: () => void
    onBindEndpoint: () => void
}) {
    const canBindEndpoint = conn.allowedActions.includes(
        "BIND_ENDPOINT_REFERENCE",
    )
    const canBindCredential = conn.allowedActions.includes(
        "BIND_CREDENTIAL_REFERENCE",
    )
    const endpointBlocker = conn.actionBlockers.find(
        (blocker) => blocker.action === "BIND_ENDPOINT_REFERENCE",
    )
    const credentialBlocker = conn.actionBlockers.find(
        (blocker) => blocker.action === "BIND_CREDENTIAL_REFERENCE",
    )
    return (
        <div className="space-y-3">
            <Alert>
                <KeyRoundIcon aria-hidden="true" />
                <AlertTitle>安全配置引用</AlertTitle>
                <AlertDescription>
                    仅显示绑定状态、安全别名与版本。永不展示、复制或导出密钥正文。轮换只能选择密钥管理系统不透明引用。
                </AlertDescription>
            </Alert>
            <div className="grid gap-3 sm:grid-cols-2">
                <Card
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                        <CardTitle className="text-base">
                            地址配置引用
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm">
                        <RefLabel
                            state={conn.safeReferences.endpoint.state}
                            alias={conn.safeReferences.endpoint.alias}
                            version={conn.safeReferences.endpoint.version}
                            visible={conn.safeReferences.endpoint.visible}
                        />
                        {canBindEndpoint || endpointBlocker ? (
                            <Button
                                type="button"
                                size="sm"
                                disabled={!canBindEndpoint}
                                title={endpointBlocker?.message}
                                onClick={onBindEndpoint}
                            >
                                绑定/轮换地址
                            </Button>
                        ) : null}
                    </CardContent>
                </Card>
                <Card
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                        <CardTitle className="text-base">
                            密钥配置引用
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm">
                        <RefLabel
                            state={conn.safeReferences.credential.state}
                            alias={conn.safeReferences.credential.alias}
                            version={conn.safeReferences.credential.version}
                            visible={conn.safeReferences.credential.visible}
                        />
                        {canBindCredential || credentialBlocker ? (
                            <Button
                                type="button"
                                size="sm"
                                disabled={!canBindCredential}
                                title={credentialBlocker?.message}
                                onClick={onBind}
                            >
                                绑定/轮换引用
                            </Button>
                        ) : null}
                    </CardContent>
                </Card>
            </div>
        </div>
    )
}
