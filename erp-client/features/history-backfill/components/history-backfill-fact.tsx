import * as React from "react"

export function HistoryBackfillFact({
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
            <div className={mono ? "num font-mono text-sm" : "text-sm"}>
                {value}
            </div>
        </div>
    )
}
