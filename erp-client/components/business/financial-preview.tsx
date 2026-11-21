import type { ReactNode } from "react"
import { MoneyValue } from "./values"

/** 往来预览首屏只突出一个决策金额，比较量置于下方。 */
export function PreviewAmount({
    label,
    value,
    children,
}: {
    label: string
    value: string | null
    children?: ReactNode
}) {
    return (
        <section className="border-b border-border pb-6">
            <p className="text-xs font-medium text-muted-foreground">{label}</p>
            <MoneyValue value={value} size="summary" className="mt-2" />
            {children ? (
                <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                    {children}
                </div>
            ) : null}
        </section>
    )
}

export function PreviewSection({
    title,
    children,
}: {
    title: string
    children: ReactNode
}) {
    return (
        <section className="space-y-3 border-b border-border pb-6 last:border-b-0 last:pb-0">
            <h3 className="text-body-compact font-medium">{title}</h3>
            {children}
        </section>
    )
}

export function PreviewFact({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="flex min-w-0 items-baseline justify-between gap-5">
            <dt className="shrink-0 text-xs text-muted-foreground">{label}</dt>
            <dd className="min-w-0 break-words text-right text-body-compact">
                {children ?? "—"}
            </dd>
        </div>
    )
}

export function PreviewNote({ children }: { children: ReactNode }) {
    return <p className="text-xs text-muted-foreground">{children}</p>
}

/** 已过账往来记录的统一限制说明。 */
export function PostedRecordNote() {
    return (
        <PreviewNote>
            已入账记录不可直接修改或删除；如需更正，请办理冲销或退款。
        </PreviewNote>
    )
}
