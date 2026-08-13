"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { PublicationPublishGate } from "@/features/product-publications/types"

export function PublishGateAlert({ gate }: { gate: PublicationPublishGate }) {
    if (gate.kind === "READY") {
        return (
            <Alert variant="info">
                <AlertTitle>可提交发布</AlertTitle>
                <AlertDescription>
                    {gate.priceOrTaxChanged
                        ? "价格/税率有变化且复核已满足，可提交发布。"
                        : "价格/税率无变化，可直接提交发布。"}
                </AlertDescription>
            </Alert>
        )
    }
    if (gate.kind === "REVIEW_POLICY_UNCONFIGURED") {
        return (
            <Alert variant="warning" role="alert">
                <AlertTitle>复核政策未配置</AlertTitle>
                <AlertDescription>{gate.blocker.message}</AlertDescription>
            </Alert>
        )
    }
    if (gate.kind === "RECOVERY_RESPONSIBILITY_UNCONFIRMED") {
        return (
            <Alert variant="destructive" role="alert">
                <AlertTitle>恢复责任未确认</AlertTitle>
                <AlertDescription>{gate.blocker.message}</AlertDescription>
            </Alert>
        )
    }
    return (
        <Alert variant="warning" role="alert">
            <AlertTitle>发布复核阻断</AlertTitle>
            <AlertDescription>{gate.blocker.message}</AlertDescription>
        </Alert>
    )
}
