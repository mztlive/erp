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
    /**
     * 标题行与右侧动作的垂直对齐。缺省：compact 居中，default 顶对齐。
     */
    titleRowAlign?: "start" | "center"
}

const metaSlotClassName =
    "min-w-0 text-xs leading-5 text-muted-foreground [&_svg:not([class*='size-'])]:size-3.5"

function PageHeader({
    title,
    description,
    status,
    metadata,
    actions,
    density = "compact",
    variant = "page",
    titleRowAlign,
    className,
    ...props
}: PageHeaderProps) {
    const objectChrome = variant === "object-chrome"
    const compact = objectChrome || density === "compact"
    const rowAlign = titleRowAlign ?? (compact ? "center" : "start")
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
                // 白壳：与侧栏同色，压在浅色内容区之上，吸顶时发丝底边托住滚上来的卡片。
                "sticky top-0 z-20 flex shrink-0 flex-col border-b border-border bg-card",
                objectChrome ? "gap-1.5" : compact ? "gap-1.5" : "gap-2.5",
                className,
            )}
            {...props}
        >
            {objectChrome && (metadata || actions) ? (
                <div className="flex min-h-0 flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                    {metadata ? (
                        <div className={metaSlotClassName}>{metadata}</div>
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
                        rowAlign === "center"
                            ? "lg:items-center"
                            : "lg:items-start",
                    )}
                >
                    <div className="min-w-0 flex-1">
                        <div
                            className={cn(
                                "flex flex-wrap items-center",
                                compact ? "gap-x-2.5 gap-y-1" : "gap-2.5",
                            )}
                        >
                            {title != null ? (
                                <h1
                                    className={cn(
                                        "font-semibold tracking-tight text-foreground",
                                        compact
                                            ? "text-xl leading-7"
                                            : "text-2xl leading-8",
                                    )}
                                >
                                    {title}
                                </h1>
                            ) : null}
                            {status ? <StatusBadge {...status} /> : null}
                            {compact && metadata ? (
                                <div className={metaSlotClassName}>
                                    {metadata}
                                </div>
                            ) : null}
                        </div>
                        {description ? (
                            <p
                                className={cn(
                                    "max-w-2xl text-sm leading-relaxed text-muted-foreground",
                                    compact ? "mt-1" : "mt-1.5",
                                )}
                            >
                                {description}
                            </p>
                        ) : null}
                        {!compact && metadata ? (
                            <div className={cn("mt-3", metaSlotClassName)}>
                                {metadata}
                            </div>
                        ) : null}
                    </div>
                    {actions ? (
                        <div
                            className={cn(
                                "shrink-0 lg:ml-auto",
                                !compact && "lg:pt-0.5",
                            )}
                        >
                            {actions}
                        </div>
                    ) : null}
                </div>
            ) : !objectChrome && actions ? (
                <div className="flex justify-end">{actions}</div>
            ) : null}
        </header>
    )
}

/**
 * 页头约束/口径提示行。描边胶囊，不用灰底，避免和说明文字糊成一层。
 */
function PageHeaderMeta({ className, ...props }: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="page-header-meta"
            className={cn("flex flex-wrap items-center gap-1.5", className)}
            {...props}
        />
    )
}

function PageHeaderMetaItem({
    className,
    ...props
}: React.ComponentProps<"span">) {
    return (
        <span
            data-slot="page-header-meta-item"
            className={cn(
                "inline-flex items-center gap-1.5 rounded-md border border-border/80 bg-background px-2 py-0.5 text-xs text-muted-foreground [&_svg]:size-3.5",
                className,
            )}
            {...props}
        />
    )
}

export { PageHeader, PageHeaderMeta, PageHeaderMetaItem }
