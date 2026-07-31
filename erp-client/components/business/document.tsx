"use client"

import * as React from "react"
import {
  ChevronDownIcon,
  Clock3Icon,
  HistoryIcon,
  LockIcon,
  UserRoundIcon,
} from "lucide-react"

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
  StatusBadge,
  type StatusTone,
} from "@/components/ui/status-badge"
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

interface DocumentHeaderProps
  extends Omit<React.ComponentProps<"header">, "title"> {
  title: string
  documentNumber: string
  primaryStatus: DocumentStatus
  statuses?: readonly DocumentStatusTrack[]
  version?: string | number
  primaryAction?: React.ReactNode
  secondaryActions?: React.ReactNode
}

function DocumentHeader({
  title,
  documentNumber,
  primaryStatus,
  statuses = [],
  version,
  primaryAction,
  secondaryActions,
  className,
  ...props
}: DocumentHeaderProps) {
  const hasActions = primaryAction != null || secondaryActions != null

  return (
    <header
      data-slot="document-header"
      className={cn("border-b border-border pb-5", className)}
      {...props}
    >
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="min-w-0 space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="font-heading text-2xl font-semibold tracking-tight">
              {title}
            </h1>
            <StatusBadge
              tone={primaryStatus.tone}
              label={primaryStatus.label}
            />
          </div>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
            <span>
              单号 <span className="num text-foreground">{documentNumber}</span>
            </span>
            {version != null ? (
              <span className="num rounded-md bg-muted px-2 py-1 text-xs text-foreground">
                版本 {version}
              </span>
            ) : null}
          </div>
        </div>

        {hasActions ? (
          <div
            data-slot="document-header-actions"
            className="flex shrink-0 flex-wrap items-center gap-2"
          >
            {secondaryActions}
            {primaryAction}
          </div>
        ) : null}
      </div>

      {statuses.length > 0 ? (
        <div
          role="list"
          aria-label="单据并行状态"
          className="mt-4 flex flex-wrap gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3"
        >
          {statuses.map((track) => (
            <div
              key={track.id}
              role="listitem"
              className="flex items-center gap-2"
            >
              <span className="text-xs font-medium text-muted-foreground">
                {track.label}
              </span>
              <StatusBadge
                tone={track.status.tone}
                label={track.status.label}
              />
            </div>
          ))}
        </div>
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

interface DocumentSummaryProps
  extends Omit<React.ComponentProps<"section">, "children"> {
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
      className={cn("rounded-lg border border-border bg-card p-5", className)}
      {...props}
    >
      <DescriptionList columns={columns}>
        {items.map((item) => (
          <DescriptionItem key={item.id}>
            <DescriptionTerm>{item.label}</DescriptionTerm>
            <DescriptionDetails
              className={cn(
                item.numeric && "num",
                item.emphasized && "font-medium"
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

interface DocumentSectionProps
  extends Omit<React.ComponentProps<"section">, "children" | "title"> {
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
        <div className="mt-1 text-sm text-muted-foreground">{description}</div>
      ) : null}
    </div>
  )

  return (
    <section
      data-slot="document-section"
      className={cn("border-b border-border py-5 last:border-b-0", className)}
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
          <CollapsibleContent className="pt-4">{children}</CollapsibleContent>
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

interface RevisionTimelineProps
  extends Omit<React.ComponentProps<"div">, "children"> {
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
                  <span className="num">版本 {revision.version}</span>
                  {revision.isCurrent ? (
                    <StatusBadge tone="info" label="当前版本" />
                  ) : null}
                  {revision.status != null ? (
                    <StatusBadge
                      tone={revision.status.tone}
                      label={revision.status.label}
                    />
                  ) : null}
                </TimelineTitle>
                <TimelineTime dateTime={revision.effectiveAt.dateTime}>
                  {revision.effectiveAt.label}
                </TimelineTime>
              </TimelineHeader>
              <TimelineDescription>
                <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
                  <span>来源：{revisionSourceLabels[revision.source]}</span>
                  <span>操作人：{revision.actor}</span>
                </div>
                {revision.reason != null ? (
                  <div className="mt-2 text-foreground">{revision.reason}</div>
                ) : null}
                {revision.action != null ? (
                  <div className="mt-3">{revision.action}</div>
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
  measure: RelatedDocumentMeasure
  owner: string
  openAction: React.ReactNode
}>

interface RelatedDocumentListProps
  extends Omit<React.ComponentProps<"div">, "children"> {
  documents: readonly RelatedDocument[]
  emptyContent?: React.ReactNode
}

function RelatedDocumentList({
  documents,
  emptyContent = "暂无关联单据",
  className,
  ...props
}: RelatedDocumentListProps) {
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
            className="hidden grid-cols-12 gap-3 border-b border-border pb-2 text-xs font-medium text-muted-foreground md:grid"
          >
            <span className="col-span-4">单据</span>
            <span className="col-span-2">状态</span>
            <span className="col-span-2">金额或数量</span>
            <span className="col-span-2">责任人</span>
            <span className="col-span-2 text-right">操作</span>
          </div>
          <ul className="divide-y divide-border">
            {documents.map((document) => {
              const measureLabel =
                document.measure.label ??
                (document.measure.kind === "amount" ? "金额" : "数量")

              return (
                <li
                  key={document.id}
                  className="grid grid-cols-1 gap-3 py-4 first:pt-3 last:pb-0 md:grid-cols-12 md:items-center"
                >
                  <div className="min-w-0 md:col-span-4">
                    <div className="text-xs text-muted-foreground">
                      {document.documentType}
                    </div>
                    <div className="num truncate text-sm font-medium">
                      {document.documentNumber}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 md:col-span-2">
                    <span className="text-xs text-muted-foreground md:hidden">
                      状态
                    </span>
                    <StatusBadge
                      tone={document.status.tone}
                      label={document.status.label}
                    />
                  </div>
                  <div className="md:col-span-2">
                    <div className="text-xs text-muted-foreground">
                      {measureLabel}
                    </div>
                    <div className="num text-sm font-medium">
                      {document.measure.value}
                      {document.measure.kind === "quantity" &&
                      document.measure.unit != null ? (
                        <span className="ml-1 font-normal text-muted-foreground">
                          {document.measure.unit}
                        </span>
                      ) : null}
                    </div>
                  </div>
                  <div className="md:col-span-2">
                    <div className="text-xs text-muted-foreground md:hidden">
                      责任人
                    </div>
                    <div className="truncate text-sm">{document.owner}</div>
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

type ResponsibilityBlocker = Readonly<{
  label?: string
  reason: React.ReactNode
  tone?: Extract<StatusTone, "warning" | "destructive">
}>

type ResponsibilityTrackBase = Readonly<{
  id: string
  label: string
  description?: React.ReactNode
  status: DocumentStatus
  owner: string
  dueAt?: DisplayTime
  blocker?: ResponsibilityBlocker
}>

type ResponsibilityTrackResolution =
  | Readonly<{
      action: React.ReactNode
      disabledReason?: never
    }>
  | Readonly<{
      action?: never
      disabledReason: React.ReactNode
    }>

type ResponsibilityTrack = ResponsibilityTrackBase &
  ResponsibilityTrackResolution

interface ResponsibilityPanelProps
  extends Omit<React.ComponentProps<"section">, "children" | "title"> {
  title?: string
  description?: React.ReactNode
  tracks: readonly ResponsibilityTrack[]
  emptyContent?: React.ReactNode
}

function ResponsibilityPanel({
  title = "并行责任",
  description,
  tracks,
  emptyContent = "暂无责任轨道",
  className,
  ...props
}: ResponsibilityPanelProps) {
  return (
    <section
      data-slot="responsibility-panel"
      className={cn(
        "overflow-hidden rounded-lg border border-border bg-card",
        className
      )}
      {...props}
    >
      <div className="border-b border-border px-4 py-3">
        <h2 className="font-heading text-base font-semibold">{title}</h2>
        {description != null ? (
          <div className="mt-1 text-sm text-muted-foreground">
            {description}
          </div>
        ) : null}
      </div>

      {tracks.length > 0 ? (
        <ul className="divide-y divide-border">
          {tracks.map((track) => (
            <li key={track.id} className="p-4">
              <div className="grid gap-4 lg:grid-cols-12 lg:items-start">
                <div className="min-w-0 space-y-2 lg:col-span-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-sm font-medium">{track.label}</h3>
                    <StatusBadge
                      tone={track.status.tone}
                      label={track.status.label}
                    />
                  </div>
                  {track.description != null ? (
                    <div className="text-sm text-muted-foreground">
                      {track.description}
                    </div>
                  ) : null}
                </div>

                <dl className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:col-span-4">
                  <div>
                    <dt className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
                      <UserRoundIcon aria-hidden="true" className="size-3" />
                      责任人
                    </dt>
                    <dd className="mt-1 text-sm">{track.owner}</dd>
                  </div>
                  <div>
                    <dt className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
                      <Clock3Icon aria-hidden="true" className="size-3" />
                      期限
                    </dt>
                    <dd className="num mt-1 text-sm">
                      {track.dueAt != null ? (
                        <time dateTime={track.dueAt.dateTime}>
                          {track.dueAt.label}
                        </time>
                      ) : (
                        <span className="text-muted-foreground">未设置</span>
                      )}
                    </dd>
                  </div>
                </dl>

                <div className="lg:col-span-4">
                  {track.action != null ? (
                    <div className="flex lg:justify-end">{track.action}</div>
                  ) : (
                    <div className="flex items-start gap-2 rounded-md bg-muted px-3 py-2 text-sm text-muted-foreground lg:justify-end">
                      <LockIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0"
                      />
                      <span>{track.disabledReason}</span>
                    </div>
                  )}
                </div>
              </div>

              {track.blocker != null ? (
                <div className="mt-3 flex flex-col gap-2 rounded-md border border-border bg-surface-sunken px-3 py-2 sm:flex-row sm:items-start">
                  <StatusBadge
                    tone={track.blocker.tone ?? "destructive"}
                    label={track.blocker.label ?? "已阻塞"}
                  />
                  <div className="text-sm text-foreground">
                    {track.blocker.reason}
                  </div>
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      ) : (
        <div className="px-4 py-5 text-sm text-muted-foreground">
          {emptyContent}
        </div>
      )}
    </section>
  )
}

export {
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  RelatedDocumentList,
  ResponsibilityPanel,
  RevisionTimeline,
  type DisplayTime,
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
  type ResponsibilityBlocker,
  type ResponsibilityPanelProps,
  type ResponsibilityTrack,
  type RevisionSource,
  type RevisionTimelineEntry,
  type RevisionTimelineProps,
}
