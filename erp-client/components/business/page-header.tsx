import * as React from "react"

import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

type StatusBadgeProps = React.ComponentProps<typeof StatusBadge>

export type PageHeaderDensity = "default" | "compact"

/**
 * page：工作台/列表/治理等「页面级」标题头。
 * object-chrome：M4 对象中心壳层，只保留轻动作与 metadata；对象身份交给 DocumentHeader，
 * 禁止再叠一层与 DocumentHeader 重复的大标题。
 */
export type PageHeaderVariant = "page" | "object-chrome"

export type PageHeaderProps = Omit<
    React.ComponentProps<"header">,
    "children" | "title"
> & {
    /**
     * page 变体必填；object-chrome 时可选（通常省略，避免与 DocumentHeader 双标题）。
     */
    title?: React.ReactNode
    description?: React.ReactNode
    status?: Pick<StatusBadgeProps, "label" | "tone" | "icon">
    metadata?: React.ReactNode
    actions?: React.ReactNode
    /**
     * compact（默认）：标题、状态与 metadata 同排，供高频作业页压缩首屏。
     * default：标题 text-2xl、metadata 独占一行，用于需要展示语气的落地页。
     * object-chrome 变体始终按 compact 节奏渲染，本 prop 仅影响 page 变体。
     */
    density?: PageHeaderDensity
    /**
     * page（默认）：列表/工作台页头。
     * object-chrome：对象中心导航条（可选 metadata/返回），不渲染 h1 工作面名。
     */
    variant?: PageHeaderVariant
}

function PageHeader({
    title,
    description,
    status,
    metadata,
    actions,
    density = "compact",
    variant = "page",
    className,
    ...props
}: PageHeaderProps) {
    const objectChrome = variant === "object-chrome"
    const compact = objectChrome || density === "compact"
    const showTitleBlock =
        !objectChrome &&
        (title != null ||
            status != null ||
            description != null ||
            metadata != null)

    return (
        <header
            data-slot="page-header"
            data-density={objectChrome ? "compact" : density}
            data-variant={variant}
            className={cn(
                "sticky top-0 z-20 flex shrink-0 flex-col border-b border-border bg-card",
                objectChrome ? "gap-1.5" : compact ? "gap-2" : "gap-3",
                className,
            )}
            {...props}
        >
            {objectChrome && (metadata || actions) ? (
                <div className="flex min-h-0 flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                    {metadata ? (
                        <div className="min-w-0 text-xs text-muted-foreground">
                            {metadata}
                        </div>
                    ) : null}
                    {actions ? (
                        <div className="shrink-0 sm:ml-auto">{actions}</div>
                    ) : null}
                </div>
            ) : null}

            {showTitleBlock ? (
                <div
                    className={cn(
                        "flex flex-col gap-3 lg:flex-row",
                        compact ? "lg:items-center" : "lg:items-start",
                    )}
                >
                    <div className="min-w-0 flex-1">
                        <div
                            className={cn(
                                "flex flex-wrap items-center",
                                compact ? "gap-x-3 gap-y-1" : "gap-2",
                            )}
                        >
                            {title != null ? (
                                <h1
                                    className={cn(
                                        "font-semibold tracking-tight text-foreground",
                                        compact ? "text-xl" : "text-2xl",
                                    )}
                                >
                                    {title}
                                </h1>
                            ) : null}
                            {status ? <StatusBadge {...status} /> : null}
                            {compact && metadata ? (
                                <div className="min-w-0 text-sm text-muted-foreground">
                                    {metadata}
                                </div>
                            ) : null}
                        </div>
                        {description ? (
                            <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
                                {description}
                            </p>
                        ) : null}
                        {!compact && metadata ? (
                            <div className="mt-2 text-xs text-muted-foreground [&_svg:not([class*='size-'])]:size-3.5">
                                {metadata}
                            </div>
                        ) : null}
                    </div>
                    {actions ? (
                        <div className="shrink-0 lg:ml-auto">{actions}</div>
                    ) : null}
                </div>
            ) : !objectChrome && actions ? (
                <div className="flex justify-end">{actions}</div>
            ) : null}
        </header>
    )
}

export { PageHeader }
