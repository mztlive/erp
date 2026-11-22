import * as React from "react"

import { StatusBadge } from "@/components/ui/status-badge"

export function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div className="space-y-0.5">
            <div className="text-xs text-muted-foreground">{label}</div>
            <div
                className={
                    mono
                        ? "num font-mono text-body-compact"
                        : "text-body-compact"
                }
            >
                {value}
            </div>
        </div>
    )
}

export function GateRow({ ok, label }: { ok: boolean; label: string }) {
    return (
        <div className="flex items-start gap-2 text-body-compact">
            <StatusBadge
                tone={ok ? "success" : "destructive"}
                label={ok ? "已满足" : "未满足"}
            />
            <span className={ok ? "text-foreground" : "text-muted-foreground"}>
                {label}
            </span>
        </div>
    )
}
