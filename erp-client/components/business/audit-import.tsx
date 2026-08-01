"use client";

import * as React from "react";
import {
  CircleCheckIcon,
  CircleDashedIcon,
  CircleXIcon,
  HistoryIcon,
  LoaderCircleIcon,
} from "lucide-react";

import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@/components/ui/item";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Timeline,
  TimelineDescription,
  TimelineHeader,
  TimelineItem,
  TimelineMarker,
  TimelineTime,
  TimelineTitle,
} from "@/components/ui/timeline";
import { cn } from "@/lib/utils";

export type WorkTaskStatus = Readonly<{
  label: string;
  tone?: StatusTone;
}>;

export type WorkTaskItemDensity = "default" | "compact";

export interface WorkTaskItemProps extends Omit<
  React.ComponentProps<typeof Item>,
  "children" | "title"
> {
  taskType: React.ReactNode;
  businessObject: React.ReactNode;
  counterparty?: React.ReactNode;
  enteredAt: React.ReactNode;
  enteredDateTime?: string;
  dueAt: React.ReactNode;
  dueDateTime?: string;
  responsibleParty: React.ReactNode;
  reason: React.ReactNode;
  impact: React.ReactNode;
  status?: WorkTaskStatus;
  nextAction?: React.ReactNode;
  /**
   * compact：只保留任务类型、对象、截止与责任方，省略进入队列时间与原因/影响。
   * 仅用于右侧已有详情面板的队列选择列表；独立展示的工作台请用 default。
   */
  density?: WorkTaskItemDensity;
}

/**
 * 工作台任务条目。截止时间、责任方和可执行动作均由调用方提供，组件不判断逾期或权限。
 */
export function WorkTaskItem({
  taskType,
  businessObject,
  counterparty,
  enteredAt,
  enteredDateTime,
  dueAt,
  dueDateTime,
  responsibleParty,
  reason,
  impact,
  status,
  nextAction,
  density = "default",
  className,
  ...props
}: WorkTaskItemProps) {
  const compact = density === "compact";
  const due = dueDateTime ? (
    <time dateTime={dueDateTime}>{dueAt}</time>
  ) : (
    dueAt
  );

  return (
    <Item
      data-slot="work-task-item"
      data-density={density}
      variant="outline"
      size={compact ? "xs" : "sm"}
      className={cn("items-start", className)}
      {...props}
    >
      <ItemContent>
        <ItemTitle className="flex-wrap">
          <span>{taskType}</span>
          {status ? (
            <StatusBadge label={status.label} tone={status.tone} />
          ) : null}
        </ItemTitle>
        <ItemDescription className="flex flex-wrap gap-x-3 gap-y-1">
          <span className="font-medium text-foreground">{businessObject}</span>
          {counterparty ? <span>{counterparty}</span> : null}
        </ItemDescription>

        {compact ? (
          <dl className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs">
            <div className="flex items-baseline gap-1">
              <dt className="text-muted-foreground">截止</dt>
              <dd className="font-medium text-foreground">{due}</dd>
            </div>
            <div className="flex min-w-0 items-baseline gap-1">
              <dt className="sr-only">责任方</dt>
              <dd className="truncate text-muted-foreground">
                {responsibleParty}
              </dd>
            </div>
          </dl>
        ) : (
          <>
            <dl className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-xs">
              <div className="flex items-baseline gap-1">
                <dt className="text-muted-foreground">进入队列</dt>
                <dd className="font-medium text-foreground">
                  {enteredDateTime ? (
                    <time dateTime={enteredDateTime}>{enteredAt}</time>
                  ) : (
                    enteredAt
                  )}
                </dd>
              </div>
              <div className="flex items-baseline gap-1">
                <dt className="text-muted-foreground">截止</dt>
                <dd className="font-medium text-foreground">{due}</dd>
              </div>
              <div className="flex items-baseline gap-1">
                <dt className="text-muted-foreground">责任方</dt>
                <dd className="font-medium text-foreground">
                  {responsibleParty}
                </dd>
              </div>
            </dl>
            <dl className="mt-2 grid gap-1 text-xs">
              <div className="flex items-start gap-1">
                <dt className="shrink-0 text-muted-foreground">原因</dt>
                <dd className="text-foreground">{reason}</dd>
              </div>
              <div className="flex items-start gap-1">
                <dt className="shrink-0 text-muted-foreground">影响</dt>
                <dd className="text-foreground">{impact}</dd>
              </div>
            </dl>
          </>
        )}
      </ItemContent>
      {nextAction ? <ItemActions>{nextAction}</ItemActions> : null}
    </Item>
  );
}

export type BusinessDiffEntry = Readonly<{
  id: React.Key;
  field: React.ReactNode;
  before: React.ReactNode;
  after: React.ReactNode;
  note?: React.ReactNode;
}>;

export interface BusinessDiffPanelProps extends Omit<
  React.ComponentProps<"section">,
  "children" | "title"
> {
  changes: readonly BusinessDiffEntry[];
  title?: React.ReactNode;
  caption?: string;
  emptyValue?: React.ReactNode;
  emptyMessage?: React.ReactNode;
}

/**
 * 字段级差异面板。敏感字段的脱敏策略由调用方在 before/after 节点中显式决定。
 */
export function BusinessDiffPanel({
  changes,
  title = "字段变更",
  caption = "字段变更前后值对比",
  emptyValue = "—",
  emptyMessage = "没有字段变更",
  className,
  ...props
}: BusinessDiffPanelProps) {
  return (
    <section
      data-slot="business-diff-panel"
      className={cn("overflow-hidden rounded-2xl border bg-card", className)}
      {...props}
    >
      <header className="flex items-center justify-between gap-3 border-b px-4 py-3">
        <h3 className="text-sm font-semibold text-card-foreground">{title}</h3>
        <Badge variant="neutral">{changes.length} 项</Badge>
      </header>
      <Table>
        <TableCaption className="sr-only">{caption}</TableCaption>
        <TableHeader>
          <TableRow>
            <TableHead>字段</TableHead>
            <TableHead>变更前</TableHead>
            <TableHead>变更后</TableHead>
            <TableHead>变更说明</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {changes.length > 0 ? (
            changes.map((change) => (
              <TableRow key={change.id}>
                <TableCell className="font-medium text-foreground">
                  {change.field}
                </TableCell>
                <TableCell className="whitespace-normal text-muted-foreground">
                  {change.before ?? emptyValue}
                </TableCell>
                <TableCell className="whitespace-normal text-foreground">
                  {change.after ?? emptyValue}
                </TableCell>
                <TableCell className="whitespace-normal text-muted-foreground">
                  {change.note ?? emptyValue}
                </TableCell>
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={4}
                className="text-center text-muted-foreground"
              >
                {emptyMessage}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </section>
  );
}

export type AuditTimelineEntry = Readonly<{
  id: React.Key;
  action: React.ReactNode;
  operator: React.ReactNode;
  occurredAt: string;
  occurredAtLabel: React.ReactNode;
  source: React.ReactNode;
  note?: React.ReactNode;
  marker?: React.ReactNode;
}>;

export interface AuditTimelineProps extends Omit<
  React.ComponentProps<typeof Timeline>,
  "children"
> {
  entries: readonly AuditTimelineEntry[];
  emptyMessage?: React.ReactNode;
}

/** 审计事实时间线。组件不格式化时间，也不推导操作者或来源。 */
export function AuditTimeline({
  entries,
  emptyMessage = "暂无审计记录",
  className,
  "aria-label": ariaLabel = "审计记录",
  ...props
}: AuditTimelineProps) {
  if (entries.length === 0) {
    return (
      <p
        data-slot="audit-timeline-empty"
        role="status"
        className={cn("text-sm text-muted-foreground", className)}
      >
        {emptyMessage}
      </p>
    );
  }

  return (
    <Timeline
      data-slot="audit-timeline"
      aria-label={ariaLabel}
      className={className}
      {...props}
    >
      {entries.map((entry) => (
        <TimelineItem key={entry.id}>
          <TimelineMarker>
            {entry.marker ?? <HistoryIcon aria-hidden="true" />}
          </TimelineMarker>
          <TimelineHeader>
            <TimelineTitle>{entry.action}</TimelineTitle>
            <TimelineTime dateTime={entry.occurredAt}>
              {entry.occurredAtLabel}
            </TimelineTime>
          </TimelineHeader>
          <TimelineDescription>
            <span className="flex flex-wrap gap-x-4 gap-y-1">
              <span>
                操作者：
                <span className="text-foreground">{entry.operator}</span>
              </span>
              <span>
                来源：<span className="text-foreground">{entry.source}</span>
              </span>
            </span>
            {entry.note ? (
              <span className="mt-1 block text-foreground">{entry.note}</span>
            ) : null}
          </TimelineDescription>
        </TimelineItem>
      ))}
    </Timeline>
  );
}

export type ImportStageKey =
  "upload" | "mapping" | "validation" | "preview" | "submission" | "result";

export type ImportStageStatus = "pending" | "current" | "complete" | "failed";

export type ImportStageDisplay = Readonly<{
  status: ImportStageStatus;
  description?: React.ReactNode;
}>;

export type ImportStageStates = Readonly<
  Record<ImportStageKey, ImportStageDisplay>
>;

export interface ImportStageIndicatorProps extends Omit<
  React.ComponentProps<"ol">,
  "children"
> {
  stages: ImportStageStates;
}

const importStages: ReadonlyArray<{
  key: ImportStageKey;
  label: string;
}> = [
  { key: "upload", label: "上传" },
  { key: "mapping", label: "映射" },
  { key: "validation", label: "校验" },
  { key: "preview", label: "预览" },
  { key: "submission", label: "提交" },
  { key: "result", label: "结果" },
];

const importStatusPresentation: Readonly<
  Record<
    ImportStageStatus,
    {
      label: string;
      tone: StatusTone;
      icon: typeof CircleDashedIcon;
    }
  >
> = {
  pending: {
    label: "未开始",
    tone: "neutral",
    icon: CircleDashedIcon,
  },
  current: {
    label: "进行中",
    tone: "info",
    icon: LoaderCircleIcon,
  },
  complete: {
    label: "已完成",
    tone: "success",
    icon: CircleCheckIcon,
  },
  failed: {
    label: "失败",
    tone: "destructive",
    icon: CircleXIcon,
  },
};

/** 固定顺序展示导入流程；每一步的事实状态由调用方显式传入。 */
export function ImportStageIndicator({
  stages,
  className,
  "aria-label": ariaLabel = "导入进度",
  ...props
}: ImportStageIndicatorProps) {
  return (
    <ol
      data-slot="import-stage-indicator"
      aria-label={ariaLabel}
      className={cn(
        "grid overflow-hidden rounded-2xl border bg-card divide-y divide-border lg:grid-cols-6 lg:divide-x lg:divide-y-0",
        className,
      )}
      {...props}
    >
      {importStages.map((stage) => {
        const stageDisplay = stages[stage.key];
        const presentation = importStatusPresentation[stageDisplay.status];

        return (
          <li
            key={stage.key}
            aria-current={
              stageDisplay.status === "current" ? "step" : undefined
            }
            className="flex min-w-0 flex-col items-start gap-2 p-3"
          >
            <span className="text-sm font-medium text-card-foreground">
              {stage.label}
            </span>
            <StatusBadge
              label={presentation.label}
              tone={presentation.tone}
              icon={presentation.icon}
            />
            {stageDisplay.description ? (
              <span className="text-xs text-muted-foreground">
                {stageDisplay.description}
              </span>
            ) : null}
          </li>
        );
      })}
    </ol>
  );
}

export type ImportIssue = Readonly<{
  id: React.Key;
  rowNumber: number | string;
  field: string;
  errorCode: string;
  message: React.ReactNode;
  repairable: boolean;
}>;

export interface ImportIssueTableProps extends Omit<
  React.ComponentProps<"div">,
  "children"
> {
  issues: readonly ImportIssue[];
  caption?: string;
  emptyMessage?: React.ReactNode;
}

/** 导入问题明细。可修复标识直接展示调用方事实，不内置修复动作。 */
export function ImportIssueTable({
  issues,
  caption = "导入问题明细",
  emptyMessage = "没有导入问题",
  className,
  ...props
}: ImportIssueTableProps) {
  return (
    <div
      data-slot="import-issue-table"
      className={cn("overflow-hidden rounded-2xl border bg-card", className)}
      {...props}
    >
      <Table>
        <TableCaption className="sr-only">{caption}</TableCaption>
        <TableHeader>
          <TableRow>
            <TableHead>行号</TableHead>
            <TableHead>字段</TableHead>
            <TableHead>错误码</TableHead>
            <TableHead>说明</TableHead>
            <TableHead>修复方式</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {issues.length > 0 ? (
            issues.map((issue) => (
              <TableRow key={issue.id}>
                <TableCell className="num">{issue.rowNumber}</TableCell>
                <TableCell className="font-medium text-foreground">
                  {issue.field}
                </TableCell>
                <TableCell className="num text-muted-foreground">
                  {issue.errorCode}
                </TableCell>
                <TableCell className="whitespace-normal">
                  {issue.message}
                </TableCell>
                <TableCell>
                  <StatusBadge
                    label={issue.repairable ? "可在导入中修复" : "需外部处理"}
                    tone={issue.repairable ? "info" : "destructive"}
                  />
                </TableCell>
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={5}
                className="text-center text-muted-foreground"
              >
                {emptyMessage}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  );
}

export type BatchOperationResultItem = Readonly<{
  id: React.Key;
  label: React.ReactNode;
  detail?: React.ReactNode;
  code?: string;
}>;

export interface BatchOperationResultProps extends Omit<
  React.ComponentProps<"section">,
  "children" | "title"
> {
  succeeded: readonly BatchOperationResultItem[];
  skipped?: readonly BatchOperationResultItem[];
  failed: readonly BatchOperationResultItem[];
  title?: React.ReactNode;
  retryAction?: React.ReactNode;
  successEmptyMessage?: React.ReactNode;
  skippedEmptyMessage?: React.ReactNode;
  failureEmptyMessage?: React.ReactNode;
}

/** 批处理分项结果。重试行为由调用方注入，组件不重新提交请求。 */
export function BatchOperationResult({
  succeeded,
  skipped = [],
  failed,
  title = "批处理结果",
  retryAction,
  successEmptyMessage = "没有成功项",
  skippedEmptyMessage = "没有跳过项",
  failureEmptyMessage = "没有失败项",
  className,
  ...props
}: BatchOperationResultProps) {
  const titleId = React.useId();

  return (
    <section
      data-slot="batch-operation-result"
      aria-labelledby={titleId}
      className={cn("space-y-3", className)}
      {...props}
    >
      <header className="flex flex-wrap items-center gap-2">
        <h3 id={titleId} className="text-sm font-semibold text-foreground">
          {title}
        </h3>
        <Badge variant="success">成功 {succeeded.length}</Badge>
        <Badge variant="neutral">跳过 {skipped.length}</Badge>
        <Badge variant={failed.length > 0 ? "destructive" : "neutral"}>
          失败 {failed.length}
        </Badge>
      </header>
      <div className="grid gap-3 lg:grid-cols-2 xl:grid-cols-3">
        <Alert variant="success">
          <CircleCheckIcon aria-hidden="true" />
          <AlertTitle>成功项</AlertTitle>
          <AlertDescription>
            {succeeded.length > 0 ? (
              <ul className="mt-2 space-y-2">
                {succeeded.map((item) => (
                  <li key={item.id}>
                    <span className="font-medium">{item.label}</span>
                    {item.code ? (
                      <Badge variant="success" className="ml-2">
                        {item.code}
                      </Badge>
                    ) : null}
                    {item.detail ? (
                      <span className="mt-1 block">{item.detail}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : (
              <span>{successEmptyMessage}</span>
            )}
          </AlertDescription>
        </Alert>
        <Alert>
          <CircleDashedIcon aria-hidden="true" />
          <AlertTitle>跳过项</AlertTitle>
          <AlertDescription>
            {skipped.length > 0 ? (
              <ul className="mt-2 space-y-2">
                {skipped.map((item) => (
                  <li key={item.id}>
                    <span className="font-medium">{item.label}</span>
                    {item.code ? (
                      <Badge variant="neutral" className="ml-2">
                        {item.code}
                      </Badge>
                    ) : null}
                    {item.detail ? (
                      <span className="mt-1 block">{item.detail}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : (
              <span>{skippedEmptyMessage}</span>
            )}
          </AlertDescription>
        </Alert>
        <Alert variant={failed.length > 0 ? "destructive" : "default"}>
          <CircleXIcon aria-hidden="true" />
          <AlertTitle>失败项</AlertTitle>
          <AlertDescription>
            {failed.length > 0 ? (
              <ul className="mt-2 space-y-2">
                {failed.map((item) => (
                  <li key={item.id}>
                    <span className="font-medium">{item.label}</span>
                    {item.code ? (
                      <Badge variant="destructive" className="ml-2">
                        {item.code}
                      </Badge>
                    ) : null}
                    {item.detail ? (
                      <span className="mt-1 block">{item.detail}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : (
              <span>{failureEmptyMessage}</span>
            )}
          </AlertDescription>
          {retryAction ? <AlertAction>{retryAction}</AlertAction> : null}
        </Alert>
      </div>
    </section>
  );
}
