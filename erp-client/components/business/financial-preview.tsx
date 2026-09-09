import type { ReactNode } from "react"
import { MoneyValue } from "./values"

/** 往来预览首屏只突出一个决策金额，比较量置于下方。 */
export function PreviewAmount({
    label,
    value,
    children,
}: {
    label: string
    value: string
    children?: ReactNode
}) {
    return (
        <section className="border-b border-border pb-6">
            <p className="text-xs font-medium text-muted-foreground">{label}</p>
            <MoneyValue
                value={value}
                className="mt-2 [&>span:first-child]:text-[32px] [&>span:first-child]:font-semibold [&>span:first-child]:tracking-tight"
            />
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
            <h3 className="text-sm font-medium">{title}</h3>
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
            <dd className="min-w-0 break-words text-right text-[13px]">
                {children ?? "—"}
            </dd>
        </div>
    )
}

export function PreviewNote({ children }: { children: ReactNode }) {
    return <p className="text-xs leading-5 text-muted-foreground">{children}</p>
}
