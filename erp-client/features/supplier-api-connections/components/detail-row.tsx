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
        <div className="flex items-start justify-between gap-3">
            <dt className="shrink-0 text-muted-foreground">{label}</dt>
            <dd className={mono ? "font-mono text-right" : "text-right"}>
                {value}
            </dd>
        </div>
    )
}
