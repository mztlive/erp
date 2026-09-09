"use client"

import type * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon } from "lucide-react"

import type { DocumentHeaderProps } from "./document"
import { Button, buttonVariants } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

export type DetailPageHeaderProps = Omit<
    DocumentHeaderProps,
    "density" | "documentNumber" | "primaryStatus"
> & {
    documentNumber?: string
    numberLabel?: string
    primaryStatus?: DocumentHeaderProps["primaryStatus"]
    back?: { id: string; label: string } & (
        | { href: string; onClick?: never }
        | { href?: never; onClick: () => void }
    )
    navigationMeta?: React.ReactNode
    media?: React.ReactNode
    headingProps?: React.ComponentProps<"h1">
}

/** 详情页共用商品页的返回行、对象身份行与主操作布局。 */
export function DetailPageHeader({
    title,
    back,
    navigationMeta,
    media,
    headingProps,
    documentNumber,
    numberLabel = "单号",
    primaryStatus,
    statuses = [],
    version,
    meta,
    titleExtra,
    primaryAction,
    secondaryActions,
    summary,
    children,
    className,
    ...props
}: DetailPageHeaderProps) {
    return (
        <header
            data-slot="detail-page-header"
            className={cn("min-w-0 space-y-4", className)}
            {...props}
        >
            {back || navigationMeta || secondaryActions ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex min-w-0 flex-wrap items-center gap-3">
                        {back?.href != null ? (
                            <Link
                                id={back.id}
                                href={back.href}
                                className={cn(
                                    buttonVariants({
                                        variant: "ghost",
                                        size: "sm",
                                    }),
                                    "-ml-2 text-muted-foreground",
                                )}
                            >
                                <ArrowLeftIcon aria-hidden />
                                {back.label}
                            </Link>
                        ) : back ? (
                            <Button
                                id={back.id}
                                type="button"
                                variant="ghost"
                                size="sm"
                                className="-ml-2 text-muted-foreground"
                                onClick={back.onClick}
                            >
                                <ArrowLeftIcon aria-hidden />
                                {back.label}
                            </Button>
                        ) : null}
                        {navigationMeta ? (
                            <div className="min-w-0 text-sm text-muted-foreground">
                                {navigationMeta}
                            </div>
                        ) : null}
                    </div>
                    {secondaryActions ? (
                        <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                            {secondaryActions}
                        </div>
                    ) : null}
                </div>
            ) : null}
            <div className="flex flex-wrap items-center gap-4 pb-2">
                {media}
                <div className="min-w-0 flex-1 basis-56 space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                        <h1
                            {...headingProps}
                            className={cn(
                                "min-w-0 max-w-full break-words text-xl font-semibold tracking-tight md:text-2xl",
                                headingProps?.className,
                            )}
                        >
                            {title}
                        </h1>
                        {primaryStatus ? (
                            <StatusBadge {...primaryStatus} />
                        ) : null}
                        {titleExtra}
                    </div>
                    {documentNumber != null || version != null || meta ? (
                        <div className="flex flex-wrap gap-x-3 gap-y-1 text-sm text-muted-foreground [&>*]:min-w-0 [&>*]:wrap-anywhere">
                            {documentNumber != null ? (
                                <span>
                                    {numberLabel}：{documentNumber}
                                </span>
                            ) : null}
                            {version != null ? (
                                <span>版本 {version}</span>
                            ) : null}
                            {meta}
                        </div>
                    ) : null}
                </div>
                {primaryAction ? (
                    <div className="flex min-w-0 max-w-full flex-wrap items-center gap-2">
                        {primaryAction}
                    </div>
                ) : null}
            </div>
            {statuses.length > 0 ? (
                <div
                    role="list"
                    aria-label="业务状态"
                    className="flex flex-wrap items-center gap-x-4 gap-y-2"
                >
                    {statuses.map((track) => (
                        <div
                            key={track.id}
                            role="listitem"
                            className="flex items-center gap-1.5"
                        >
                            <span className="text-xs text-muted-foreground">
                                {track.label}
                            </span>
                            <StatusBadge {...track.status} />
                        </div>
                    ))}
                </div>
            ) : null}
            {summary != null ? (
                <div data-slot="detail-page-header-summary">{summary}</div>
            ) : null}
            {children ? <div className="space-y-2">{children}</div> : null}
        </header>
    )
}
