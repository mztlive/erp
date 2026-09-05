import * as React from "react"

/** 详情区键值行；标签左、值右，mono 用于代码/任务号。 */
export function Row({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div className="flex min-w-0 items-start justify-between gap-6 border-b border-border/60 py-3 last:border-b-0">
            <dt className="shrink-0 text-muted-foreground">{label}</dt>
            <dd
                className={
                    mono
                        ? "min-w-0 break-all text-right font-mono"
                        : "min-w-0 break-words text-right"
                }
            >
                {value}
            </dd>
        </div>
    )
}
