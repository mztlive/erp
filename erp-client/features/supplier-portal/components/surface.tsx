"use client"
import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
export function PortalSurface({
    title,
    description,
    actions,
    children,
}: {
    title: string
    description?: string
    actions?: ReactNode
    children: ReactNode
}) {
    return (
        <section className="mx-auto flex w-full max-w-7xl flex-col gap-6 p-4 md:p-8">
            <header className="flex flex-wrap items-start justify-between gap-4">
                <div>
                    <h1 className="text-2xl font-semibold">{title}</h1>
                    {description && (
                        <p className="mt-2 text-sm text-muted-foreground">
                            {description}
                        </p>
                    )}
                </div>
                <div className="flex flex-wrap gap-2">{actions}</div>
            </header>
            {children}
        </section>
    )
}
export function PortalError({
    error,
    retry,
    id = "supplier-portal-retry",
}: {
    error: unknown
    retry?: () => void
    id?: string
}) {
    if (!error) return null
    return (
        <div
            role="alert"
            className="rounded-lg border border-destructive/30 p-4 text-sm text-destructive"
        >
            <p>{getErrorMessage(error, "操作未完成，请核对后重试")}</p>
            {retry && (
                <Button
                    id={id}
                    variant="outline"
                    className="mt-2"
                    onClick={retry}
                >
                    重试
                </Button>
            )}
        </div>
    )
}
export function PortalPaging({
    page,
    total,
    onChange,
    prefix,
}: {
    page: number
    total: number
    onChange: (page: number) => void
    prefix: string
}) {
    return (
        <div className="flex items-center justify-end gap-3 text-sm">
            <span>
                共 {total} 条 · 第 {page} 页
            </span>
            <Button
                id={`${prefix}-previous`}
                variant="outline"
                disabled={page <= 1}
                onClick={() => onChange(page - 1)}
            >
                上一页
            </Button>
            <Button
                id={`${prefix}-next`}
                variant="outline"
                disabled={page * 50 >= total}
                onClick={() => onChange(page + 1)}
            >
                下一页
            </Button>
        </div>
    )
}
