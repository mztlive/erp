"use client"

import * as React from "react"
import { ChevronDownIcon, HistoryIcon } from "lucide-react"

import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    StatusTrackSummary,
    taxAmountToneClass,
} from "@/components/business/values"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import { cn } from "@/lib/utils"

type DocumentStatus = Readonly<{
    label: string
    tone: StatusTone
}>

type DocumentStatusTrack = Readonly<{
    id: string
    label: string
    status: DocumentStatus
}>

type DocumentHeaderDensity = "default" | "compact"

interface DocumentHeaderProps extends Omit<
    React.ComponentProps<"header">,
    "title"
> {
    title: string
    documentNumber: string
    primaryStatus: DocumentStatus
    statuses?: readonly DocumentStatusTrack[]
    version?: string | number
    /**
     * 身份元信息：负责销售、协作摘要等，渲染在单号/版本同一行。
     * 避免把长句塞进 secondaryActions 拉高右侧。
     */
    meta?: React.ReactNode
    /**
     * 标题行附加物：业务性质等身份标签，紧跟主状态徽章。
     * 不要把进度轨或长文案放这里。
     */
    titleExtra?: React.ReactNode
    primaryAction?: React.ReactNode
    secondaryActions?: React.ReactNode
    /**
     * compact（M4 对象中心推荐）：更小标题与间距，单号/版本/meta 同行。
     * default：正式单据阅读态，保留更大标题与状态轨留白。
     */
    density?: DocumentHeaderDensity
    /**
     * 金额/KPI 摘要。渲染在身份带下方、通栏铺开，避免宽屏并排留下大块空白。
     * 不要再把金额摘要塞进 children。
     */
    summary?: React.ReactNode
    /**
     * 身份卡内补充区：有效期、说明等，仍渲染在主带下方。
     */
    children?: React.ReactNode
}

function DocumentHeader({
    title,
    documentNumber,
    primaryStatus,
    statuses = [],
    version,
    meta,
    titleExtra,
    primaryAction,
    secondaryActions,
    density = "default",
    summary,
    className,
    children,
    ...props
}: DocumentHeaderProps) {
    const hasActions = primaryAction != null || secondaryActions != null
    const compact = density === "compact"

    const identityBlock = (
        <div
            className={cn(
                "min-w-0 flex-1",
                compact ? "space-y-1" : "space-y-2",
            )}
        >
            <div className="flex flex-wrap items-center gap-2">
                <h1
                    className={cn(
                        "font-heading font-semibold tracking-tight",
                        "text-[26px] leading-9",
                    )}
                >
                    {title}
                </h1>
                <StatusBadge
                    tone={primaryStatus.tone}
                    label={primaryStatus.label}
                />
                {titleExtra}
            </div>
            <div
                className={cn(
                    "flex flex-wrap items-center gap-x-2.5 gap-y-1 text-muted-foreground",
                    compact ? "text-xs" : "text-sm",
                )}
            >
                <span>
                    单号{" "}
                    <span className="num text-foreground">
                        {documentNumber}
                    </span>
                </span>
                {version != null ? (
                    <span
                        className={cn(
                            "num text-muted-foreground",
                            compact
                                ? "px-1.5 py-0.5 text-tiny"
                                : "px-2 py-1 text-xs",
                        )}
                    >
                        版本 {version}
                    </span>
                ) : null}
                {meta ? (
                    <span className="min-w-0 text-muted-foreground">
                        {meta}
                    </span>
                ) : null}
            </div>
        </div>
    )

    const actionBar = hasActions ? (
        <div
            data-slot="document-header-actions"
            className="flex shrink-0 flex-wrap items-center justify-end gap-2"
        >
            {secondaryActions}
            {primaryAction}
        </div>
    ) : null

    const statusRow =
        statuses.length > 0 ? (
            <div
                role="list"
                aria-label="单据并行状态"
                className={cn(
                    "flex flex-wrap items-center gap-x-4 gap-y-2",
                    compact ? "mt-3" : "mt-4",
                )}
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
                        <StatusBadge
                            tone={track.status.tone}
                            label={track.status.label}
                        />
                    </div>
                ))}
            </div>
        ) : null

    return (
        <header
            data-slot="document-header"
            data-density={density}
            className={cn(
                // 对象身份区以底部分隔线连接业务分区
                "border-b border-border bg-card",
                compact ? "pb-6" : "pb-7",
                className,
            )}
            {...props}
        >
            {/* 标题行与动作同一带；金额摘要通栏在下，避免宽屏并排留白 */}
            <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
                {identityBlock}
                {actionBar}
            </div>
            {statusRow}
            {summary != null ? (
                <div
                    data-slot="document-header-summary"
                    className={cn(
                        "border-t border-border",
                        compact ? "mt-3 pt-3" : "mt-4 pt-4",
                    )}
                >
                    {summary}
                </div>
            ) : null}

            {children ? (
                <div className={cn(compact ? "mt-3" : "mt-4")}>{children}</div>
            ) : null}
        </header>
    )
}

type DocumentSummaryColumns = "one" | "two" | "three" | "four"

type DocumentSummaryItem = Readonly<{
    id: string
    label: string
    value: React.ReactNode
    description?: React.ReactNode
    numeric?: boolean
    emphasized?: boolean
}>

interface DocumentSummaryProps extends Omit<
    React.ComponentProps<"section">,
    "children"
> {
    items: readonly DocumentSummaryItem[]
    columns?: DocumentSummaryColumns
}

function DocumentSummary({
    items,
    columns = "two",
    className,
    ...props
}: DocumentSummaryProps) {
    return (
        <section
            data-slot="document-summary"
            className={cn("border-b border-border bg-card py-5", className)}
            {...props}
        >
            <DescriptionList columns={columns}>
                {items.map((item) => (
                    <DescriptionItem key={item.id}>
                        <DescriptionTerm>{item.label}</DescriptionTerm>
                        <DescriptionDetails
                            className={cn(
                                item.numeric && "num",
                                item.emphasized && "font-medium",
                                taxAmountToneClass(item.label),
                            )}
                        >
                            {item.value}
                            {item.description != null ? (
                                <span className="mt-1 block text-xs font-normal text-muted-foreground">
                                    {item.description}
                                </span>
                            ) : null}
                        </DescriptionDetails>
                    </DescriptionItem>
                ))}
            </DescriptionList>
        </section>
    )
}

interface DocumentSectionProps extends Omit<
    React.ComponentProps<"section">,
    "children" | "title"
> {
    title: string
    description?: React.ReactNode
    action?: React.ReactNode
    children: React.ReactNode
    collapsible?: boolean
    defaultOpen?: boolean
}

function DocumentSection({
    title,
    description,
    action,
    children,
    collapsible = false,
    defaultOpen = true,
    className,
    ...props
}: DocumentSectionProps) {
    const heading = (
        <div className="min-w-0">
            <h2 className="font-heading text-base font-semibold">{title}</h2>
            {description != null ? (
                <div className="mt-1 text-sm text-muted-foreground">
                    {description}
                </div>
            ) : null}
        </div>
    )

    return (
        <section
            data-slot="document-section"
            className={cn(
                "border-b border-grid py-5 last:border-b-0",
                className,
            )}
            {...props}
        >
            {collapsible ? (
                <Collapsible defaultOpen={defaultOpen}>
                    <div className="flex items-start justify-between gap-4">
                        {heading}
                        <div className="flex shrink-0 items-center gap-2">
                            {action}
                            <CollapsibleTrigger
                                className="group inline-flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                aria-label={`展开或收起${title}`}
                            >
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className="size-4 transition-transform group-aria-expanded:rotate-180"
                                />
                            </CollapsibleTrigger>
                        </div>
                    </div>
                    <CollapsibleContent className="pt-4">
                        {children}
                    </CollapsibleContent>
                </Collapsible>
            ) : (
                <>
                    <div className="flex items-start justify-between gap-4">
                        {heading}
                        {action != null ? (
                            <div className="shrink-0">{action}</div>
                        ) : null}
                    </div>
                    <div className="pt-4">{children}</div>
                </>
            )}
        </section>
    )
}

type RevisionSource =
    | "mall-sync"
    | "erp-change"
    | "migration-baseline"
    | "system-correction"

const revisionSourceLabels = {
    "mall-sync": "商城同步",
    "erp-change": "ERP 变更",
    "migration-baseline": "迁移基线",
    "system-correction": "系统纠正",
} satisfies Record<RevisionSource, string>

type DisplayTime = Readonly<{
    dateTime: string
    label: string
}>

type RevisionTimelineEntry = Readonly<{
    id: string
    version: string | number
    source: RevisionSource
    actor: string
    effectiveAt: DisplayTime
    reason?: React.ReactNode
    status?: DocumentStatus
    isCurrent?: boolean
    action?: React.ReactNode
}>

interface RevisionTimelineProps extends Omit<
    React.ComponentProps<"div">,
    "children"
> {
    revisions: readonly RevisionTimelineEntry[]
    emptyContent?: React.ReactNode
}

function RevisionTimeline({
    revisions,
    emptyContent = "暂无版本记录",
    className,
    ...props
}: RevisionTimelineProps) {
    return (
        <div
            data-slot="revision-timeline"
            className={cn("min-w-0", className)}
            {...props}
        >
            {revisions.length > 0 ? (
                <Timeline>
                    {revisions.map((revision) => (
                        <TimelineItem key={revision.id}>
                            <TimelineMarker>
                                <HistoryIcon aria-hidden="true" />
                            </TimelineMarker>
                            <TimelineHeader>
                                <TimelineTitle className="flex flex-wrap items-center gap-2">
                                    <span className="num">
                                        版本 {revision.version}
                                    </span>
                                    {revision.isCurrent ? (
                                        <StatusBadge
                                            tone="info"
                                            label="当前版本"
                                        />
                                    ) : null}
                                    {revision.status != null ? (
                                        <StatusBadge
                                            tone={revision.status.tone}
                                            label={revision.status.label}
                                        />
                                    ) : null}
                                </TimelineTitle>
                                <TimelineTime
                                    dateTime={revision.effectiveAt.dateTime}
                                >
                                    {revision.effectiveAt.label}
                                </TimelineTime>
                            </TimelineHeader>
                            <TimelineDescription>
                                <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
                                    <span>
                                        来源：
                                        {revisionSourceLabels[revision.source]}
                                    </span>
                                    <span>操作人：{revision.actor}</span>
                                </div>
                                {revision.reason != null ? (
                                    <div className="mt-2 text-foreground">
                                        {revision.reason}
                                    </div>
                                ) : null}
                                {revision.action != null ? (
                                    <div className="mt-3">
                                        {revision.action}
                                    </div>
                                ) : null}
                            </TimelineDescription>
                        </TimelineItem>
                    ))}
                </Timeline>
            ) : (
                <div className="rounded-lg bg-muted px-4 py-3 text-sm text-muted-foreground">
                    {emptyContent}
                </div>
            )}
        </div>
    )
}

type RelatedDocumentMeasure =
    | Readonly<{
          kind: "amount"
          value: React.ReactNode
          label?: string
      }>
    | Readonly<{
          kind: "quantity"
          value: React.ReactNode
          unit?: React.ReactNode
          label?: string
      }>

type RelatedDocument = Readonly<{
    id: string
    documentType: string
    documentNumber: string
    status: DocumentStatus
    /**
     * 并行进度轨（履约/付款等）。有值时状态列加宽，供销售等角色扫单据是否卡住。
     */
    tracks?: readonly DocumentStatusTrack[]
    measure: RelatedDocumentMeasure
    owner: string
    openAction: React.ReactNode
}>

interface RelatedDocumentListProps extends Omit<
    React.ComponentProps<"div">,
    "children"
> {
    documents: readonly RelatedDocument[]
    emptyContent?: React.ReactNode
}

function RelatedDocumentList({
    documents,
    emptyContent = "暂无关联单据",
    className,
    ...props
}: RelatedDocumentListProps) {
    const hasTracks = documents.some(
        (document) => (document.tracks?.length ?? 0) > 0,
    )

    return (
        <div
            data-slot="related-document-list"
            className={cn("min-w-0", className)}
            {...props}
        >
            {documents.length > 0 ? (
                <>
                    <div
                        aria-hidden="true"
                        className="hidden grid-cols-12 gap-3 border-b border-grid pb-2 text-xs font-medium text-muted-foreground md:grid"
                    >
                        <span
                            className={hasTracks ? "col-span-3" : "col-span-4"}
                        >
                            单据
                        </span>
                        <span
                            className={hasTracks ? "col-span-4" : "col-span-2"}
                        >
                            {hasTracks ? "状态 / 进度" : "状态"}
                        </span>
                        <span className="col-span-2">金额或数量</span>
                        <span
                            className={hasTracks ? "col-span-1" : "col-span-2"}
                        >
                            责任人
                        </span>
                        <span className="col-span-2 text-right">操作</span>
                    </div>
                    <ul className="divide-y divide-grid">
                        {documents.map((document) => {
                            const measureLabel =
                                document.measure.label ??
                                (document.measure.kind === "amount"
                                    ? "金额"
                                    : "数量")
                            const tracks = document.tracks ?? []

                            return (
                                <li
                                    key={document.id}
                                    className="grid grid-cols-1 gap-3 py-4 first:pt-3 last:pb-0 md:grid-cols-12 md:items-center"
                                >
                                    <div
                                        className={cn(
                                            "min-w-0",
                                            hasTracks
                                                ? "md:col-span-3"
                                                : "md:col-span-4",
                                        )}
                                    >
                                        <div className="text-xs text-muted-foreground">
                                            {document.documentType}
                                        </div>
                                        <div className="num truncate text-sm font-medium">
                                            {document.documentNumber}
                                        </div>
                                    </div>
                                    <div
                                        className={cn(
                                            "min-w-0",
                                            hasTracks
                                                ? "md:col-span-4"
                                                : "flex items-center gap-2 md:col-span-2",
                                        )}
                                    >
                                        <span className="mb-1 block text-xs text-muted-foreground md:hidden">
                                            {hasTracks ? "状态 / 进度" : "状态"}
                                        </span>
                                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                                            <StatusBadge
                                                tone={document.status.tone}
                                                label={document.status.label}
                                            />
                                            {tracks.length > 0 ? (
                                                <StatusTrackSummary
                                                    variant="inline"
                                                    className="gap-x-3 gap-y-1"
                                                    tracks={tracks}
                                                />
                                            ) : null}
                                        </div>
                                    </div>
                                    <div className="md:col-span-2">
                                        <div className="text-xs text-muted-foreground">
                                            {measureLabel}
                                        </div>
                                        <div
                                            className={cn(
                                                "num text-sm font-medium",
                                                taxAmountToneClass(
                                                    measureLabel,
                                                ),
                                            )}
                                        >
                                            {document.measure.value}
                                            {document.measure.kind ===
                                                "quantity" &&
                                            document.measure.unit != null ? (
                                                <span className="ml-1 font-normal text-muted-foreground">
                                                    {document.measure.unit}
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>
                                    <div
                                        className={
                                            hasTracks
                                                ? "md:col-span-1"
                                                : "md:col-span-2"
                                        }
                                    >
                                        <div className="text-xs text-muted-foreground md:hidden">
                                            责任人
                                        </div>
                                        <div className="truncate text-sm">
                                            {document.owner}
                                        </div>
                                    </div>
                                    <div className="flex md:col-span-2 md:justify-end">
                                        {document.openAction}
                                    </div>
                                </li>
                            )
                        })}
                    </ul>
                </>
            ) : (
                <div className="rounded-lg bg-muted px-4 py-3 text-sm text-muted-foreground">
                    {emptyContent}
                </div>
            )}
        </div>
    )
}

export {
    DocumentHeader,
    DocumentSection,
    DocumentSummary,
    RelatedDocumentList,
    RevisionTimeline,
    type DisplayTime,
    type DocumentHeaderDensity,
    type DocumentHeaderProps,
    type DocumentSectionProps,
    type DocumentStatus,
    type DocumentStatusTrack,
    type DocumentSummaryColumns,
    type DocumentSummaryItem,
    type DocumentSummaryProps,
    type RelatedDocument,
    type RelatedDocumentListProps,
    type RelatedDocumentMeasure,
    type RevisionSource,
    type RevisionTimelineEntry,
    type RevisionTimelineProps,
}
