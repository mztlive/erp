"use client"

import { type ReactNode } from "react"
import { CircleHelpIcon, RefreshCwIcon } from "lucide-react"

import {
    useWorkspaceTaskNavigation,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import {
    UnifiedTaskWorkbenchRailToggle,
    useUnifiedTaskWorkbenchRail,
} from "@/components/business/unified-task-workbench"
import { IconActionButton } from "@/components/business"
import { cn } from "@/lib/utils"
import { TaskTimeline } from "@/features/document-timeline/components/task-timeline"
import { timelineDocumentTypes } from "@/features/document-timeline/types"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Item, ItemContent, ItemGroup } from "@/components/ui/item"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { Separator } from "@/components/ui/separator"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { WORK_ITEM_STATUS_META } from "@/lib/task-status-dictionary"

import type { WorkspaceWorkItem } from "../types"
import { isBlockedWorkItem } from "../lib/work-item"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceDescriptionText } from "./workspace-description-text"
import { WorkspaceTaskInstructions } from "./workspace-task-instructions"

const PRIORITY_LABEL: Record<number, string> = {
    1: "紧急",
    2: "高",
    3: "普通",
    4: "低",
}

function responsibilityLabel(item: WorkspaceWorkItem): string {
    return [
        item.ownerRoleLabel,
        item.ownerUserLabel,
        item.ownerOrganizationLabel,
    ]
        .filter((value) => value.trim())
        .join(" · ")
}

/** 说明弹层里的一条到达原因、影响或下一步，用浅底卡片和正文分开。 */
function ContextFact({ label, value }: { label: string; value: string }) {
    return (
        <Item variant="muted" size="sm">
            <ItemContent>
                <span className="text-xs text-muted-foreground">{label}</span>
                <WorkspaceDescriptionText text={value} />
            </ItemContent>
        </Item>
    )
}

/**
 * 任务说明问号。点开后展示统一任务上下文，不单独占一块说明区。
 *
 * @param item 当前工作台任务，字段来自服务端 WorkItem 投影
 */
export function WorkspaceTaskContextHelp({
    item,
    label,
    variant = "ghost",
    triggerId,
}: {
    item: WorkspaceWorkItem
    label?: string
    variant?: "ghost" | "outline"
    triggerId?: string
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const priorityLabel = PRIORITY_LABEL[item.priority] ?? `P${item.priority}`
    const resolvedTriggerId =
        triggerId ??
        `workspace-task-context-help-trigger-${toAutomationIdSegment(item.workItemId)}`
    const blockedMeta =
        item.processingState === "APPROVAL_BLOCKED"
            ? WORK_ITEM_STATUS_META.APPROVAL_BLOCKED
            : WORK_ITEM_STATUS_META.EXECUTION_BLOCKED

    return (
        <Popover>
            <PopoverTrigger
                id={resolvedTriggerId}
                render={
                    <Button
                        type="button"
                        variant={variant}
                        className={
                            variant === "outline" ? "cursor-pointer" : undefined
                        }
                        size={label ? "sm" : "icon-sm"}
                        aria-label={label || "任务说明"}
                        data-testid={`work-item-task-context-${item.workItemId}`}
                    />
                }
            >
                <CircleHelpIcon aria-hidden="true" />
                {label}
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="max-h-[min(42rem,80dvh)] w-80 overflow-y-auto sm:w-96"
                aria-label="任务责任与处理要求"
            >
                <PopoverHeader className="gap-2">
                    <div className="flex items-start justify-between gap-3">
                        <div className="flex min-w-0 flex-col gap-1">
                            <PopoverTitle>任务说明</PopoverTitle>
                            <PopoverDescription>
                                {item.workItemTypeLabel}
                            </PopoverDescription>
                        </div>
                        <StatusBadge
                            label={
                                item.status !== "OPEN"
                                    ? item.statusLabel
                                    : blocked
                                      ? blockedMeta.label
                                      : overdue
                                        ? "已超期"
                                        : WORK_ITEM_STATUS_META.OPEN.label
                            }
                            tone={
                                item.status !== "OPEN"
                                    ? item.statusTone
                                    : blocked
                                      ? blockedMeta.tone
                                      : overdue
                                        ? "destructive"
                                        : WORK_ITEM_STATUS_META.OPEN.tone
                            }
                        />
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                        <WorkspaceDocumentBadge item={item} />
                        <span className="text-xs text-muted-foreground">
                            优先级：{priorityLabel}
                        </span>
                    </div>
                </PopoverHeader>

                <ItemGroup className="gap-2">
                    <ContextFact label="为什么到你" value={item.reasonLabel} />
                </ItemGroup>
                <WorkspaceTaskInstructions item={item} />

                <Separator />

                <DescriptionList
                    columns="two"
                    className="grid-cols-2 gap-x-4 gap-y-3"
                >
                    <DescriptionItem className="col-span-2">
                        <DescriptionTerm>当前责任</DescriptionTerm>
                        <DescriptionDetails>
                            {responsibilityLabel(item) || "责任人待确认"}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>进入工作台</DescriptionTerm>
                        <DescriptionDetails>
                            {item.enteredAtLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>处理时限</DescriptionTerm>
                        <DescriptionDetails>
                            {item.dueAt ? item.dueAtLabel : "未设置截止时间"}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
            </PopoverContent>
        </Popover>
    )
}

/**
 * 作业面标题栏右侧动作。问号固定放在打开原单据等图标旁边。
 *
 * @param item 当前工作台任务
 * @param children 查看单据、打开原单据、转交等动作
 */
export function WorkspaceTaskHeaderActions({
    item,
    contextLabel,
    contextVariant,
    children,
}: {
    item: WorkspaceWorkItem
    children?: ReactNode
    contextLabel?: string
    contextVariant?: "ghost" | "outline"
}) {
    const fullPage = useWorkspaceTaskPage()
    const navigation = useWorkspaceTaskNavigation()
    const rail = useUnifiedTaskWorkbenchRail()
    const hideTimelineSheet = Boolean(rail?.hasRail)
    return (
        <div className="flex max-w-full flex-wrap items-center justify-end gap-1">
            {children}
            {fullPage &&
                !hideTimelineSheet &&
                timelineDocumentTypes.has(item.businessObjectType) && (
                    <TaskTimeline
                        key={item.workItemId}
                        taskId={item.workItemId}
                        approval={Boolean(item.approval)}
                        instance={item.workItemType === "APPROVAL_INSTANCE"}
                        title={item.counterpartyName || item.objectTitle}
                        document={{
                            type: item.businessObjectType,
                            id: item.businessObjectId,
                            number: item.stableNumber,
                        }}
                    />
                )}
            {fullPage && navigation?.onRefresh ? (
                <IconActionButton
                    id="workspace-processing-refresh"
                    testId="workspace-processing-refresh"
                    label="刷新当前任务"
                    disabled={navigation.refreshing}
                    onClick={navigation.onRefresh}
                >
                    <RefreshCwIcon
                        aria-hidden="true"
                        className={cn(navigation.refreshing && "animate-spin")}
                    />
                </IconActionButton>
            ) : null}
            <WorkspaceTaskContextHelp
                item={item}
                label={contextLabel}
                variant={contextVariant}
            />
            <UnifiedTaskWorkbenchRailToggle />
        </div>
    )
}
