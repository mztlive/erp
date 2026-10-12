import type { ReactNode } from "react"

/** 改单弹窗的一块内容：标题说明这块管什么，右侧放本块操作。 */
export function SalesChangeSection({
    id,
    title,
    description,
    action,
    children,
}: {
    id: string
    title: string
    description?: ReactNode
    action?: ReactNode
    children: ReactNode
}) {
    return (
        <section
            aria-labelledby={`${id}-title`}
            className="min-w-0 space-y-4 border-t border-grid pt-6 first:border-t-0 first:pt-0"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <h3
                        id={`${id}-title`}
                        className="font-heading text-base font-semibold"
                    >
                        {title}
                    </h3>
                    {description ? (
                        <p className="text-xs leading-relaxed text-muted-foreground">
                            {description}
                        </p>
                    ) : null}
                </div>
                {action}
            </div>
            {children}
        </section>
    )
}
