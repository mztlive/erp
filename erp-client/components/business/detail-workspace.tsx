"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business/detail-workspace-styles"
import type {
    DocumentStatus,
    DocumentStatusTrack,
} from "@/components/business/document"
import { PageScaffold } from "@/components/business/page"
import { useWorkspaceTaskNavigation } from "@/components/business/workspace-task-pane"
import { useWorkspaceTab } from "@/components/layout/workspace-tab-context"
import { Button, buttonVariants } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export { detailWorkspaceStyles }

const styles = detailWorkspaceStyles

export type DetailWorkspaceBackLink = {
    id: string
    label: string
    /**
     * `standalone-only`（默认）：仅独立打开时展示，在多页签工作台内自动隐藏；
     * `always`：即使在工作页签内也保留展示（适用于带明确 `returnTo` 上下文或深链返回）。
     */
    visibility?: "standalone-only" | "always"
} & ({ href: string; onClick?: never } | { href?: never; onClick: () => void })

function findScrollContainer(node: HTMLElement | null): HTMLElement | null {
    let current = node?.parentElement ?? null
    while (current) {
        const overflowY = window.getComputedStyle(current).overflowY
        if (overflowY === "auto" || overflowY === "scroll") {
            return current
        }
        current = current.parentElement
    }
    return null
}

export type DetailWorkspaceTabItem = Readonly<{
    id: string
    label: React.ReactNode
    title?: string
    badge?: React.ReactNode
    triggerId?: string
}>

/**
 * 详情页元信息行：统一 12px 灰字 + 自动插入极淡 `·` 分隔符，
 * 过滤掉 falsy 项，避免页面手写空格拼接导致视觉松散。
 */
export function DetailWorkspaceMeta({
    items,
    className,
}: {
    items: readonly (React.ReactNode | false | null | undefined)[]
    className?: string
}) {
    const visible = items.filter(
        (item): item is React.ReactNode =>
            item !== null &&
            item !== undefined &&
            item !== false &&
            item !== "",
    )
    if (visible.length === 0) return null
    return (
        <div className={cn(styles.meta, className)}>
            {visible.map((item, index) => (
                <React.Fragment key={index}>
                    {index > 0 ? (
                        <span
                            aria-hidden="true"
                            className="select-none text-muted-foreground/40"
                        >
                            ·
                        </span>
                    ) : null}
                    <span className="min-w-0 wrap-anywhere">{item}</span>
                </React.Fragment>
            ))}
        </div>
    )
}

/**
 * 未保存修改微胶囊：复用列表页 `● 待查询` 的柔和琥珀色胶囊语言。
 */
export function DetailWorkspaceDirtyBadge({
    label = "有未保存修改",
    className,
}: {
    label?: string
    className?: string
}) {
    return (
        <span
            role="status"
            className={cn(
                "inline-flex items-center gap-1.5 rounded-full bg-warning-soft px-2 py-0.5 text-tiny leading-4 font-medium text-warning-soft-foreground",
                className,
            )}
        >
            <span
                aria-hidden="true"
                className="size-1.5 shrink-0 rounded-full bg-warning"
            />
            {label}
        </span>
    )
}

/**
 * 详情工作面一体化吸顶纯白通栏分区导航：
 * 置于 `DetailWorkspaceChrome` 底部，与页头共用纯白表面与底线，
 * 激活蓝条压盖在通栏底线上，左右外沿严格对齐 `--scaffold-px`。
 */
export function DetailWorkspaceTabs({
    value,
    onValueChange,
    items,
    ariaLabel = "详情分区",
    tabIdPrefix,
    className,
}: {
    value: string
    onValueChange: (value: string) => void
    items: readonly DetailWorkspaceTabItem[]
    ariaLabel?: string
    tabIdPrefix?: string
    className?: string
}) {
    if (items.length < 2) return null
    return (
        <Tabs
            value={value}
            onValueChange={(next) => {
                if (next) onValueChange(next)
            }}
            className={cn("gap-0", className)}
        >
            <TabsList
                variant="line"
                aria-label={ariaLabel}
                className={styles.tabsList}
            >
                {items.map((item) => (
                    <TabsTrigger
                        key={item.id}
                        id={
                            item.triggerId ??
                            (tabIdPrefix
                                ? `${tabIdPrefix}-${toAutomationIdSegment(item.id)}`
                                : undefined)
                        }
                        value={item.id}
                        title={item.title}
                        className={styles.tab}
                    >
                        {item.label}
                        {item.badge != null ? item.badge : null}
                    </TabsTrigger>
                ))}
            </TabsList>
        </Tabs>
    )
}

export type DetailWorkspaceChromeProps = {
    title: string
    headingProps?: React.ComponentProps<"h1">
    back?: DetailWorkspaceBackLink
    navigationMeta?: React.ReactNode
    media?: React.ReactNode
    documentNumber?: string
    numberLabel?: string
    version?: string | number
    primaryStatus?: DocumentStatus
    statuses?: readonly DocumentStatusTrack[]
    titleExtra?: React.ReactNode
    meta?: React.ReactNode
    secondaryActions?: React.ReactNode
    primaryAction?: React.ReactNode
    summary?: React.ReactNode
    tabs?: React.ReactNode
    children?: React.ReactNode
    className?: string
}

/**
 * 详情工作面纯白通栏吸顶头部：
 * - 将返回行（工作页签内自动隐藏）、身份信息、操作动作与通栏分区 Tabs 融为一体；
 * - 静止态极净扁平无阴影，滚动发生遮挡时自动浮现极轻底影（`data-scrolled="true"`）。
 */
export function DetailWorkspaceChrome({
    title,
    headingProps,
    back: backNavigation,
    navigationMeta,
    media,
    documentNumber,
    numberLabel = "单号",
    version,
    primaryStatus,
    statuses = [],
    titleExtra,
    meta,
    secondaryActions,
    primaryAction,
    summary,
    tabs,
    children,
    className,
}: DetailWorkspaceChromeProps) {
    const inWorkspaceTab = useWorkspaceTab()
    const taskNavigation = useWorkspaceTaskNavigation()
    const effectiveBack = backNavigation ?? taskNavigation?.back
    const effectiveNavigationMeta = navigationMeta ?? taskNavigation?.meta
    const back =
        inWorkspaceTab && effectiveBack?.visibility !== "always"
            ? undefined
            : effectiveBack
    const sentinelRef = React.useRef<HTMLDivElement>(null)
    const headerRef = React.useRef<HTMLElement>(null)
    const [scrolled, setScrolled] = React.useState(false)

    React.useEffect(() => {
        const sentinel = sentinelRef.current
        if (!sentinel || typeof IntersectionObserver === "undefined") return
        const root = findScrollContainer(sentinel)
        const observer = new IntersectionObserver(
            ([entry]) => {
                setScrolled(!entry.isIntersecting)
            },
            { root, threshold: [0, 1] },
        )
        observer.observe(sentinel)
        return () => observer.disconnect()
    }, [])

    React.useEffect(() => {
        const header = headerRef.current
        if (!header || typeof ResizeObserver === "undefined") return
        const target =
            header.closest<HTMLElement>('[data-slot="page-scaffold"]') ??
            header.parentElement
        if (!target) return
        const syncHeight = () => {
            const height = Math.ceil(header.getBoundingClientRect().height)
            if (height > 0) {
                target.style.setProperty(
                    "--detail-chrome-height",
                    `${height + 12}px`,
                )
            }
        }
        syncHeight()
        const observer = new ResizeObserver(syncHeight)
        observer.observe(header)
        return () => {
            observer.disconnect()
            target.style.removeProperty("--detail-chrome-height")
        }
    }, [])

    const hasInlineMeta =
        documentNumber != null || version != null || meta != null

    return (
        <>
            <div
                ref={sentinelRef}
                aria-hidden="true"
                className="pointer-events-none -mt-px h-px w-full shrink-0"
            />
            <header
                ref={headerRef}
                data-slot="detail-workspace-chrome"
                data-scrolled={scrolled ? "true" : undefined}
                className={cn(styles.chrome, className)}
            >
                {back || effectiveNavigationMeta ? (
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 pt-2.5">
                        {back?.href != null ? (
                            <Link
                                id={back.id}
                                href={back.href}
                                className={cn(
                                    buttonVariants({
                                        variant: "ghost",
                                        size: "xs",
                                    }),
                                    "-ml-2 text-muted-foreground hover:text-foreground",
                                )}
                            >
                                <ArrowLeftIcon aria-hidden="true" />
                                {back.label}
                            </Link>
                        ) : back ? (
                            <Button
                                id={back.id}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="-ml-2 text-muted-foreground hover:text-foreground"
                                onClick={back.onClick}
                            >
                                <ArrowLeftIcon aria-hidden="true" />
                                {back.label}
                            </Button>
                        ) : null}
                        {effectiveNavigationMeta ? (
                            <div className="min-w-0 text-xs text-muted-foreground">
                                {effectiveNavigationMeta}
                            </div>
                        ) : null}
                    </div>
                ) : null}

                <div
                    className={cn(
                        styles.header,
                        tabs && "pb-2.5",
                        (back || effectiveNavigationMeta) && "pt-2",
                    )}
                >
                    <div className="flex min-w-0 flex-1 items-center gap-3">
                        {media}
                        <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-2">
                                <h1
                                    {...headingProps}
                                    className={cn(
                                        styles.title,
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
                            {hasInlineMeta ? (
                                <div className={styles.meta}>
                                    {documentNumber != null ? (
                                        <span
                                            className="num"
                                            data-workspace-document-number={
                                                documentNumber
                                            }
                                        >
                                            {numberLabel}：{documentNumber}
                                        </span>
                                    ) : null}
                                    {documentNumber != null &&
                                    version != null ? (
                                        <span
                                            aria-hidden="true"
                                            className="select-none text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                    ) : null}
                                    {version != null ? (
                                        <span className="num">
                                            版本 {version}
                                        </span>
                                    ) : null}
                                    {(documentNumber != null ||
                                        version != null) &&
                                    meta != null ? (
                                        <span
                                            aria-hidden="true"
                                            className="select-none text-muted-foreground/40"
                                        >
                                            ·
                                        </span>
                                    ) : null}
                                    {meta}
                                </div>
                            ) : null}
                        </div>
                    </div>

                    {secondaryActions || primaryAction ? (
                        <div
                            data-slot="detail-page-header-actions"
                            className={styles.headerActions}
                        >
                            {secondaryActions}
                            {primaryAction}
                        </div>
                    ) : null}
                </div>

                {statuses.length > 0 ? (
                    <div
                        role="list"
                        aria-label="业务状态"
                        className="flex flex-wrap items-center gap-x-4 gap-y-1.5 pb-2.5"
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
                    <div
                        data-slot="detail-page-header-summary"
                        className="pb-3"
                    >
                        {summary}
                    </div>
                ) : null}

                {children ? (
                    <div className="space-y-2 pb-2.5">{children}</div>
                ) : null}

                {tabs ? (
                    <div data-slot="detail-workspace-tabs" className="mt-0.5">
                        {tabs}
                    </div>
                ) : null}
            </header>
        </>
    )
}

/**
 * 详情工作面正文分区：纯白无边框、靠顶部细线与留白建立秩序。
 * - `width="form"`：把表单区收敛到舒适阅读宽度（`max-w-4xl`），避免宽屏拉出超长空输入条；
 * - `width="full"`：宽表格/媒体画廊铺满整行。
 */
export function DetailWorkspaceSection({
    id,
    tabIndex,
    title,
    headingId,
    headingTabIndex,
    ariaLabel,
    badge,
    subtitle,
    actions,
    width = "full",
    divider,
    children,
    className,
    contentClassName,
}: {
    id?: string
    tabIndex?: number
    title?: React.ReactNode
    headingId?: string
    headingTabIndex?: number
    ariaLabel?: string
    badge?: React.ReactNode
    subtitle?: React.ReactNode
    actions?: React.ReactNode
    width?: "form" | "full"
    /** 显式控制顶部分隔线：`true` 强制显示（跨包裹容器时）；`false` 强制隐藏。 */
    divider?: boolean
    children: React.ReactNode
    className?: string
    contentClassName?: string
}) {
    const hasHeader = title != null || actions != null
    return (
        <section
            id={id}
            tabIndex={tabIndex}
            data-slot="detail-workspace-section"
            data-divider={
                divider === true
                    ? "true"
                    : divider === false
                      ? "false"
                      : undefined
            }
            aria-label={headingId ? undefined : ariaLabel}
            aria-labelledby={headingId}
            className={cn(
                styles.section,
                tabIndex != null && "outline-none",
                divider === true && "border-t border-border/60",
                divider === false && "border-t-0!",
                className,
            )}
        >
            <div
                data-section-width={width}
                className={cn(
                    "min-w-0 space-y-4",
                    width === "form" && "max-w-4xl",
                    contentClassName,
                )}
            >
                {hasHeader ? (
                    <div className={styles.sectionHeader}>
                        <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                            {title != null ? (
                                <h2
                                    id={headingId}
                                    tabIndex={headingTabIndex}
                                    className={cn(
                                        styles.sectionTitle,
                                        headingTabIndex != null &&
                                            "outline-none",
                                    )}
                                >
                                    {title}
                                </h2>
                            ) : null}
                            {badge}
                            {subtitle ? (
                                <span className={styles.sectionSubtitle}>
                                    {subtitle}
                                </span>
                            ) : null}
                        </div>
                        {actions ? (
                            <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                {actions}
                            </div>
                        ) : null}
                    </div>
                ) : null}
                {children}
            </div>
        </section>
    )
}

/**
 * 详情工作面主体容器：
 * - 单栏模式：内部 `DetailWorkspaceSection` 自动以 `border-t border-border/60` 分隔；
 * - 双栏模式（传入 `sidebar`）：在 `xl` 宽屏以 `1px` 竖线分出右侧摘要栏，零套娃卡片；
 * - 可选 `summary` 槽位：置于吸顶头部正下方、不吸顶通栏铺开（适用于单据类金额 KPI 摘要带）；
 * - 底部悬浮保存胶囊开启时自动追加底部留白，保证滚动到最末行仍完全可点。
 */
export function DetailWorkspaceBody({
    summary,
    alerts,
    sidebar,
    sidebarLabel,
    saveDockOpen = false,
    children,
    className,
}: {
    summary?: React.ReactNode
    alerts?: React.ReactNode
    sidebar?: React.ReactNode
    sidebarLabel?: string
    saveDockOpen?: boolean
    children: React.ReactNode
    className?: string
}) {
    return (
        <div
            data-slot="detail-workspace-body"
            className={cn(
                styles.body,
                saveDockOpen && "pb-28 sm:pb-20",
                className,
            )}
        >
            {summary != null ? (
                <div
                    data-slot="detail-workspace-summary"
                    className={styles.summaryBar}
                >
                    {summary}
                </div>
            ) : null}

            {sidebar != null ? (
                <div
                    data-slot="detail-workspace-split"
                    className="grid items-start gap-x-6 gap-y-0 @[960px]/workspace:grid-cols-[minmax(0,1fr)_280px] @[1120px]/workspace:grid-cols-[minmax(0,1fr)_320px] @[1480px]/workspace:grid-cols-[minmax(0,1fr)_360px]"
                >
                    <div className="min-w-0">
                        {alerts ? (
                            <div
                                data-slot="detail-workspace-alerts"
                                className="space-y-2.5 pt-4 empty:hidden"
                            >
                                {alerts}
                            </div>
                        ) : null}
                        {children}
                    </div>
                    <aside
                        aria-label={sidebarLabel}
                        className="min-w-0 border-t border-border/60 py-5 @[960px]/workspace:self-stretch @[960px]/workspace:border-t-0 @[960px]/workspace:border-l @[960px]/workspace:border-border/60 @[960px]/workspace:pl-6"
                    >
                        {sidebar}
                    </aside>
                </div>
            ) : (
                <>
                    {alerts ? (
                        <div
                            data-slot="detail-workspace-alerts"
                            className="space-y-2.5 pt-4 empty:hidden"
                        >
                            {alerts}
                        </div>
                    ) : null}
                    {children}
                </>
            )}
        </div>
    )
}

const metricsColumnsClass: Record<2 | 3 | 4, string> = {
    2: "grid-cols-1 sm:grid-cols-2 sm:[&>*:not(:nth-child(2n+1))]:border-l sm:[&>*:not(:nth-child(2n+1))]:border-border/60 sm:[&>*:not(:nth-child(2n+1))]:pl-6",
    3: "grid-cols-1 md:grid-cols-3 md:[&>*:not(:nth-child(3n+1))]:border-l md:[&>*:not(:nth-child(3n+1))]:border-border/60 md:[&>*:not(:nth-child(3n+1))]:pl-6",
    4: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 sm:max-lg:[&>*:nth-child(even)]:border-l sm:max-lg:[&>*:nth-child(even)]:border-border/60 sm:max-lg:[&>*:nth-child(even)]:pl-6 lg:[&>*:not(:nth-child(4n+1))]:border-l lg:[&>*:not(:nth-child(4n+1))]:border-border/60 lg:[&>*:not(:nth-child(4n+1))]:pl-6",
}

const factsColumnsClass: Record<2 | 3 | 4, string> = {
    2: "grid-cols-1 sm:grid-cols-2",
    3: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3",
    4: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-4",
}

/**
 * 详情工作面纯白通栏只读信息栅格：统一 12px 柔灰标签与 13px 正文值，
 * 消除全站上百处手写 `<dl>` / `Fact` 的字号、间距与缺省值差异。
 */
export function DetailWorkspaceFacts({
    columns = 3,
    ariaLabel,
    children,
    className,
}: {
    columns?: 2 | 3 | 4
    ariaLabel?: string
    children: React.ReactNode
    className?: string
}) {
    return (
        <dl
            data-slot="detail-workspace-facts"
            aria-label={ariaLabel}
            className={cn(
                "grid gap-x-6 gap-y-4",
                factsColumnsClass[columns],
                className,
            )}
        >
            {children}
        </dl>
    )
}

/** 详情工作面只读信息单项：支持数字 `.num`、跨列与统一 `—` 空值占位。 */
export function DetailWorkspaceFact({
    label,
    value,
    numeric = false,
    span = 1,
    className,
}: {
    label: React.ReactNode
    value: React.ReactNode
    numeric?: boolean
    span?: 1 | 2 | "full"
    className?: string
}) {
    const empty = value === null || value === undefined || value === ""
    return (
        <div
            data-slot="detail-workspace-fact"
            className={cn(
                "min-w-0 space-y-1",
                span === 2 && "sm:col-span-2",
                span === "full" && "col-span-full",
                className,
            )}
        >
            <dt className="text-xs font-medium text-muted-foreground">
                {label}
            </dt>
            <dd
                className={cn(
                    "min-w-0 wrap-anywhere text-body-compact text-foreground",
                    numeric && "num",
                    empty && "text-muted-foreground",
                )}
            >
                {empty ? "—" : value}
            </dd>
        </div>
    )
}

/**
 * 详情工作面无框 KPI 指标条：
 * - `variant="bar"`：用于吸顶头下方通栏金额摘要（零边框零投影，随页面自然滚走）；
 * - `variant="inline"`：用于分区顶部轻量汇总（如收款、开票、验收、附加费用合计）。
 */
export function DetailWorkspaceMetrics({
    columns = 4,
    variant = "bar",
    ariaLabel,
    children,
    className,
}: {
    columns?: 2 | 3 | 4
    variant?: "bar" | "inline"
    ariaLabel?: string
    children: React.ReactNode
    className?: string
}) {
    return (
        <div
            data-slot="detail-workspace-metrics"
            data-variant={variant}
            aria-label={ariaLabel}
            className={cn(
                "grid gap-x-6 gap-y-3.5",
                metricsColumnsClass[columns],
                variant === "bar" ? "py-3.5" : "py-1",
                className,
            )}
        >
            {children}
        </div>
    )
}

/** 详情工作面无框 KPI 单项：标签 12px 退后，主金额 24px / 辅指标 20px（均带 `.num`）。 */
export function DetailWorkspaceMetricItem({
    label,
    value,
    detail,
    emphasis = "default",
    className,
}: {
    label: React.ReactNode
    value: React.ReactNode
    detail?: React.ReactNode
    emphasis?: "primary" | "default"
    className?: string
}) {
    return (
        <div
            data-slot="detail-workspace-metric-item"
            className={cn("min-w-0", className)}
        >
            <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                {label}
            </div>
            <div
                className={cn(
                    "num mt-1 max-w-full wrap-anywhere font-semibold tracking-tight text-foreground",
                    emphasis === "primary"
                        ? "text-value-section"
                        : "text-value-compact",
                )}
            >
                {value}
            </div>
            {detail != null ? (
                <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                    {detail}
                </div>
            ) : null}
        </div>
    )
}

export type DetailWorkspaceSubTabItem = Readonly<{
    id: string
    label: React.ReactNode
    title?: string
    badge?: React.ReactNode
}>

/**
 * 详情工作面二级分段切换条：
 * 用于主分区下包含多个子业务面的场景，柔底胶囊区分于一级通栏线型 Tab 与页面操作按钮。
 */
export function DetailWorkspaceSubTabs({
    value,
    onValueChange,
    items,
    ariaLabel = "当前分区内容",
    idPrefix,
    className,
}: {
    value: string
    onValueChange: (value: string) => void
    items: readonly DetailWorkspaceSubTabItem[]
    ariaLabel?: string
    idPrefix?: string
    className?: string
}) {
    if (items.length < 2) return null
    return (
        <nav
            aria-label={ariaLabel}
            data-slot="detail-workspace-sub-tabs"
            className={cn("pt-4", className)}
        >
            <div className={styles.subTabs}>
                {items.map((item) => {
                    const active = value === item.id
                    return (
                        <button
                            key={item.id}
                            id={
                                idPrefix
                                    ? `${idPrefix}-${toAutomationIdSegment(item.id)}`
                                    : undefined
                            }
                            type="button"
                            aria-pressed={active}
                            data-active={active ? "true" : undefined}
                            title={item.title}
                            className={styles.subTab}
                            onClick={() => onValueChange(item.id)}
                        >
                            <span>{item.label}</span>
                            {item.badge != null ? item.badge : null}
                        </button>
                    )
                })}
            </div>
        </nav>
    )
}

/**
 * 详情工作面底部居中悬浮保存胶囊：
 * 与列表页 `list-selection-dock` 视觉同源，仅在表单存在未保存修改时平滑上浮，
 * 切换任意 Tab 均保持可见，避免保存入口与独立保存类 Tab 语义混淆；
 * 退场动画期间自动冻结最后一次非零变更计数，杜绝闪现 `0`。
 */
export function DetailWorkspaceSaveDock({
    open,
    changeCount,
    summary,
    description,
    actions,
    onSave,
    saving = false,
    enableSaveShortcut = true,
    className,
}: {
    open: boolean
    changeCount: number
    summary?: string
    description?: string
    actions: React.ReactNode
    /** 传入时自动挂载 `Cmd/Ctrl + S` 快捷保存（自动避开输入法与打开中的弹窗）。 */
    onSave?: () => void
    saving?: boolean
    enableSaveShortcut?: boolean
    className?: string
}) {
    const frozenRef = React.useRef({ changeCount, summary, description })
    if (open) {
        frozenRef.current = { changeCount, summary, description }
    }
    const display = open
        ? { changeCount, summary, description }
        : frozenRef.current

    React.useEffect(() => {
        if (!open || !onSave || !enableSaveShortcut) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                event.key.toLowerCase() !== "s" ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            )
                return
            event.preventDefault()
            if (!saving) onSave()
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [enableSaveShortcut, onSave, open, saving])

    return (
        <div
            data-slot="detail-save-dock"
            data-open={open ? "true" : "false"}
            aria-hidden={!open || undefined}
            className={cn(
                "pointer-events-none fixed inset-x-0 bottom-5 z-40 flex justify-center px-4",
                "transition-[opacity,transform,visibility] motion-reduce:transition-none",
                open
                    ? "visible translate-y-0 scale-100 opacity-100 duration-200 ease-out"
                    : "invisible translate-y-2.5 scale-[0.98] opacity-0 duration-150 ease-in",
                className,
            )}
        >
            <div
                data-slot="detail-save-dock-surface"
                className={cn(
                    "flex max-w-full flex-wrap items-center gap-x-3 gap-y-1.5 rounded-2xl border border-border/90 bg-popover/95 px-3 py-1.5 text-popover-foreground shadow-xl ring-1 ring-foreground/5 backdrop-blur-md dark:ring-foreground/10",
                    "[&_[data-slot=button]]:shadow-none",
                    open ? "pointer-events-auto" : "pointer-events-none",
                )}
            >
                <div
                    className="flex min-w-0 items-center gap-2 pl-1"
                    title={display.summary}
                >
                    <span
                        aria-hidden="true"
                        className="size-2 shrink-0 rounded-full bg-warning"
                    />
                    <span className="text-body-compact font-medium text-foreground">
                        {display.changeCount > 0 ? (
                            <>
                                已修改{" "}
                                <span className="num font-semibold text-primary">
                                    {display.changeCount}
                                </span>{" "}
                                项
                            </>
                        ) : (
                            "内容已修改"
                        )}
                    </span>
                    {display.description ? (
                        <span className="hidden max-w-64 truncate text-xs text-muted-foreground sm:inline">
                            · {display.description}
                        </span>
                    ) : null}
                </div>
                <span
                    aria-hidden="true"
                    className="hidden h-4 w-px bg-border/80 sm:block"
                />
                <div className="flex shrink-0 items-center gap-1.5">
                    {actions}
                </div>
            </div>
        </div>
    )
}

/**
 * 详情页首屏纯白骨架屏：避免加载期间闪出浅灰底或孤立灰块。
 */
export function DetailWorkspaceSkeleton({
    title,
    description,
    summary = false,
    sidebar = false,
}: {
    title: string
    description?: string
    summary?: boolean
    sidebar?: boolean
}) {
    const mainSkeleton = (
        <div className="max-w-4xl space-y-6 py-6" aria-busy="true">
            <div className="space-y-3">
                <div className="h-5 w-24 animate-pulse rounded bg-muted" />
                <div className="grid gap-4 sm:grid-cols-12">
                    <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-8" />
                    <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                    <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                    <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                    <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                </div>
            </div>
            <div className="border-t border-border/60 pt-6">
                <div className="h-5 w-28 animate-pulse rounded bg-muted" />
                <div className="mt-4 h-36 animate-pulse rounded-md bg-muted/70" />
            </div>
        </div>
    )
    return (
        <PageScaffold density="compact" className={styles.page}>
            <DetailWorkspaceChrome
                title={title}
                meta={description ? <span>{description}</span> : undefined}
            />
            {summary || sidebar ? (
                <DetailWorkspaceBody
                    summary={
                        summary ? (
                            <div
                                className="grid grid-cols-2 gap-6 py-3.5 lg:grid-cols-4"
                                aria-hidden="true"
                            >
                                {Array.from({ length: 4 }, (_, index) => (
                                    <div key={index} className="space-y-2">
                                        <div className="h-3.5 w-20 animate-pulse rounded bg-muted" />
                                        <div className="h-7 w-32 animate-pulse rounded bg-muted" />
                                    </div>
                                ))}
                            </div>
                        ) : undefined
                    }
                    sidebar={
                        sidebar ? (
                            <div className="space-y-4" aria-hidden="true">
                                <div className="h-5 w-28 animate-pulse rounded bg-muted" />
                                <div className="h-16 animate-pulse rounded-md bg-muted/70" />
                                <div className="h-16 animate-pulse rounded-md bg-muted/70" />
                                <div className="h-16 animate-pulse rounded-md bg-muted/70" />
                            </div>
                        ) : undefined
                    }
                >
                    {mainSkeleton}
                </DetailWorkspaceBody>
            ) : (
                mainSkeleton
            )}
        </PageScaffold>
    )
}

/**
 * 飞书风格纯白通栏详情工作面页壳（对应列表页的 `ListWorkspacePage`）。
 */
export function DetailWorkspacePage({
    children,
    className,
}: {
    children: React.ReactNode
    className?: string
}) {
    return (
        <PageScaffold density="compact" className={cn(styles.page, className)}>
            {children}
        </PageScaffold>
    )
}
