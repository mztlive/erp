"use client"
import type { ReactNode } from "react"
import { PageScaffold } from "@/components/business/page"
import { PageHeader } from "@/components/business/page-header"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
export function PortalSurface({
    title,
    description,
    actions,
    children,
    variant = "page",
}: {
    variant?: "page" | "list"
    title: string
    description?: string
    actions?: ReactNode
    children: ReactNode
}) {
    return (
        <PageScaffold
            className={
                variant === "list" ? listWorkspaceStyles.page : undefined
            }
        >
            {variant === "list" ? (
                <ListWorkspaceHeader
                    eyebrow="供应商门户"
                    title={title}
                    description={description}
                >
                    {actions ? (
                        <div className="flex flex-wrap gap-2">{actions}</div>
                    ) : null}
                </ListWorkspaceHeader>
            ) : (
                <PageHeader
                    title={title}
                    description={description}
                    actions={
                        actions ? (
                            <div className="flex flex-wrap gap-2">
                                {actions}
                            </div>
                        ) : undefined
                    }
                />
            )}
            {children}
        </PageScaffold>
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
