"use client"

import { useEffect, useState, type ReactNode } from "react"
import { CircleHelpIcon, KeyboardIcon, RefreshCwIcon } from "lucide-react"

import { IconActionButton } from "@/components/business/icon-action-button"
import {
    UnifiedTaskWorkbenchRailToggle,
    useUnifiedTaskWorkbenchRail,
} from "@/components/business/unified-task-workbench"
import {
    useWorkspaceTaskNavigation,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
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
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { WORK_ITEM_STATUS_META } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyCheatSheetToggle } from "@/lib/workspace-hotkeys"

import {
    WorkspaceDocumentBadge,
    type WorkspaceDocumentBadgeItem,
} from "./workspace-document-badge"

export type WorkspaceTaskProcessingState =
    | "READY"
    | "APPROVAL_BLOCKED"
    | "EXECUTION_BLOCKED"

export type WorkspaceTaskActionBlocker = Readonly<{
    action?: string
    code?: string
    message: string
}>

export type WorkspaceTaskApprovalContext = Readonly<{
    status?: string
}>

export type WorkspaceTaskInstructionsItem = Readonly<{
    status: string
    nextActionHint: string
    impactSummary: string
    actionBlockers: readonly WorkspaceTaskActionBlocker[]
    approval?: WorkspaceTaskApprovalContext | null
}>

export type WorkspaceTaskContextItem = Readonly<
    WorkspaceDocumentBadgeItem &
        WorkspaceTaskInstructionsItem & {
            workItemId: string
            workItemTypeLabel: string
            statusLabel: string
            statusTone: StatusTone
            processingState: WorkspaceTaskProcessingState
            priority: number
            dueAt?: string
            dueAtLabel: string
            dueBucket: "today" | "overdue" | "later"
            enteredAtLabel: string
            ownerRoleLabel: string
            ownerUserLabel: string
            ownerOrganizationLabel: string
            reasonLabel: string
        }
>

const PRIORITY_LABEL: Record<number, string> = {
    1: "紧急",
    2: "高",
    3: "普通",
    4: "低",
}

/** 判断任务是否处于审批或执行受阻态。 */
export function isBlockedWorkspaceTask(
    item: Pick<WorkspaceTaskContextItem, "processingState" | "approval">,
): boolean {
    return (
        item.processingState !== "READY" || item.approval?.status === "BLOCKED"
    )
}

function responsibilityLabel(
    item: Pick<
        WorkspaceTaskContextItem,
        "ownerRoleLabel" | "ownerUserLabel" | "ownerOrganizationLabel"
    >,
): string {
    return [
        item.ownerRoleLabel,
        item.ownerUserLabel,
        item.ownerOrganizationLabel,
    ]
        .filter((value) => value.trim())
        .join(" · ")
}

/** 系统说明按段落和分号排版；人工填写的意见只保留原有换行。 */
export function WorkspaceDescriptionText({
    text,
    className,
    splitClauses = true,
}: {
    text: string
    className?: string
    splitClauses?: boolean
}) {
    const paragraphs = text
        .trim()
        .split(/\r?\n\s*\r?\n/)
        .filter(Boolean)
    if (paragraphs.length === 0) return null

    return (
        <div
            className={cn(
                "min-w-0 space-y-2 text-body-compact wrap-anywhere",
                className,
            )}
        >
            {paragraphs.map((paragraph, paragraphIndex) => {
                // 保留标点与原文，不把逗号、金额小数或斜杠拆成独立条目。
                const clauses = splitClauses
                    ? paragraph
                          .split(/(?<=[；])\s*|\r?\n/)
                          .map((line) => line.trim())
                          .filter(Boolean)
                    : [paragraph]
                if (clauses.length === 1) {
                    return (
                        <p key={paragraphIndex} className="whitespace-pre-wrap">
                            {paragraph}
                        </p>
                    )
                }
                // 逐行说明的总述独占一行，普通“字段：值”仍随原条目展示。
                const introduction = clauses[0].match(
                    /^([^：\n]+：)\s*(第\s*\d+\s*行.+)$/,
                )
                const entries = introduction
                    ? [introduction[2], ...clauses.slice(1)]
                    : clauses
                return (
                    <div key={paragraphIndex} className="space-y-2">
                        {introduction && (
                            <p className="font-medium">{introduction[1]}</p>
                        )}
                        <ul className="list-disc space-y-2 pl-4 marker:text-muted-foreground">
                            {entries.map((entry, index) => (
                                <li
                                    key={index}
                                    className="pl-1 whitespace-pre-wrap"
                                >
                                    {entry}
                                </li>
                            ))}
                        </ul>
                    </div>
                )
            })}
        </div>
    )
}

/** 任务办理、审批和说明弹层共用同一套指引、影响与受阻信息排版。 */
export function WorkspaceTaskInstructions({
    item,
}: {
    item: WorkspaceTaskInstructionsItem
}) {
    const guide = item.nextActionHint.trim()
    const impact = item.impactSummary.trim()
    const blockers = [
        ...new Set(
            item.actionBlockers
                .map(({ message }) => message.trim())
                .filter(Boolean),
        ),
    ]
    if (!guide && !impact && blockers.length === 0) return null

    return (
        <div className="min-w-0 space-y-4">
            {guide && (
                <section className="min-w-0 space-y-2 rounded-lg bg-muted/40 p-3">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        {item.status !== "OPEN"
                            ? "办理参考"
                            : item.approval
                              ? "审批指引"
                              : "办理指引"}
                    </h4>
                    <WorkspaceDescriptionText text={guide} />
                </section>
            )}
            {impact && (
                <section className="min-w-0 space-y-2">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        影响说明
                    </h4>
                    <WorkspaceDescriptionText text={impact} />
                </section>
            )}
            {blockers.length > 0 && (
                <Alert variant="warning">
                    <AlertTitle>办理限制</AlertTitle>
                    <AlertDescription className="space-y-3">
                        {blockers.map((message) => (
                            <WorkspaceDescriptionText
                                key={message}
                                text={message}
                            />
                        ))}
                    </AlertDescription>
                </Alert>
            )}
        </div>
    )
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

export type WorkspaceTaskContextHelpProps = Readonly<{
    item: WorkspaceTaskContextItem
    label?: string
    variant?: "ghost" | "outline"
    triggerId?: string
    /** 自定义指引区内容；省略时使用 `WorkspaceTaskInstructions`。 */
    instructions?: ReactNode
}>

/**
 * 任务说明问号。点开后展示统一任务上下文，不单独占一块说明区。
 */
export function WorkspaceTaskContextHelp({
    item,
    label,
    variant = "ghost",
    triggerId,
    instructions,
}: WorkspaceTaskContextHelpProps) {
    const blocked = isBlockedWorkspaceTask(item)
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
                {instructions !== undefined ? (
                    instructions
                ) : (
                    <WorkspaceTaskInstructions item={item} />
                )}

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

export type WorkspaceTaskHeaderActionsProps = Readonly<{
    item?: WorkspaceTaskContextItem
    children?: ReactNode
    /** 完整办理页且未挂载右侧上下文轨时显示的时间线/全动态入口插槽。 */
    timelineAction?: ReactNode
    /** 自定义任务说明弹层；传 `null` 可隐藏，未传且提供 `item` 时默认渲染 `WorkspaceTaskContextHelp`。 */
    contextHelp?: ReactNode
    contextLabel?: string
    contextVariant?: "ghost" | "outline"
    className?: string
}>

/**
 * 作业面标题栏右侧动作。问号固定放在打开原单据等图标旁边，并在末尾附着右侧上下文轨开关。
 */
export function WorkspaceTaskHeaderActions({
    item,
    contextLabel,
    contextVariant,
    timelineAction,
    contextHelp,
    children,
    className,
}: WorkspaceTaskHeaderActionsProps) {
    const fullPage = useWorkspaceTaskPage()
    const navigation = useWorkspaceTaskNavigation()
    const rail = useUnifiedTaskWorkbenchRail()
    const hideTimelineSheet = Boolean(rail?.hasRail)

    return (
        <div
            className={cn(
                "flex max-w-full flex-wrap items-center justify-end gap-1",
                className,
            )}
        >
            {children}
            {fullPage && !hideTimelineSheet ? timelineAction : null}
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
            {contextHelp !== undefined ? (
                contextHelp
            ) : item ? (
                <WorkspaceTaskContextHelp
                    item={item}
                    label={contextLabel}
                    variant={contextVariant}
                />
            ) : null}
            <UnifiedTaskWorkbenchRailToggle />
        </div>
    )
}

export type WorkspaceHotkeyCheatSheetScope =
    | "queue"
    | "detail"
    | "copies-queue"
    | "copies-detail"

type HotkeyItem = Readonly<{
    keys: readonly string[]
    label: string
}>

type HotkeyGroup = Readonly<{
    heading: string
    items: readonly HotkeyItem[]
}>

function hotkeyGroupsForScope(
    scope: WorkspaceHotkeyCheatSheetScope,
): readonly HotkeyGroup[] {
    if (scope === "detail") {
        return [
            {
                heading: "队列步进",
                items: [
                    { keys: ["J", "↓"], label: "切换下一项" },
                    { keys: ["K", "↑"], label: "切换上一项" },
                ],
            },
            {
                heading: "审批决策与对照",
                items: [
                    { keys: ["A"], label: "触发通过审批（按风险分级）" },
                    { keys: ["R"], label: "展开并聚焦驳回原因" },
                    { keys: ["P"], label: "开关 / 聚焦右侧附件对照" },
                    { keys: ["H"], label: "开关 / 聚焦右侧审批轨迹" },
                    { keys: ["]"], label: "折叠 / 展开右侧上下文栏" },
                ],
            },
            {
                heading: "全局与帮助",
                items: [
                    { keys: ["⌘K"], label: "快速跳转页签、任务与菜单" },
                    { keys: ["?"], label: "打开或关闭快捷键说明" },
                ],
            },
        ]
    }
    if (scope === "copies-detail") {
        return [
            {
                heading: "连续查阅",
                items: [
                    { keys: ["J", "↓"], label: "下一条抄送" },
                    { keys: ["K", "↑"], label: "上一条抄送" },
                ],
            },
            {
                heading: "对照面板",
                items: [
                    { keys: ["P"], label: "切换至右侧附件对照" },
                    { keys: ["H"], label: "切换至右侧审批进度" },
                ],
            },
            {
                heading: "全局与帮助",
                items: [
                    { keys: ["⌘K"], label: "快速跳转页签、任务与菜单" },
                    { keys: ["?"], label: "打开或关闭快捷键说明" },
                ],
            },
        ]
    }
    if (scope === "copies-queue") {
        return [
            {
                heading: "列表与分诊",
                items: [
                    { keys: ["J", "↓"], label: "预览下一条抄送" },
                    { keys: ["K", "↑"], label: "预览上一条抄送" },
                    { keys: ["/"], label: "聚焦搜索框" },
                    { keys: ["Esc"], label: "收起检查器或清空勾选" },
                ],
            },
            {
                heading: "全局与帮助",
                items: [
                    { keys: ["⌘K"], label: "快速跳转页签、任务与菜单" },
                    { keys: ["?"], label: "打开或关闭快捷键说明" },
                ],
            },
        ]
    }
    return [
        {
            heading: "队列分诊",
            items: [
                { keys: ["J", "↓"], label: "选中下一条任务" },
                { keys: ["K", "↑"], label: "选中上一条任务" },
                { keys: ["/"], label: "聚焦搜索框" },
                { keys: ["Esc"], label: "收起检查器或清空多选" },
            ],
        },
        {
            heading: "原位快审（检查器打开时）",
            items: [
                { keys: ["A"], label: "触发通过审批（按风险分级）" },
                { keys: ["R"], label: "展开并聚焦驳回原因" },
            ],
        },
        {
            heading: "全局与帮助",
            items: [
                { keys: ["⌘K"], label: "快速跳转页签、任务与菜单" },
                { keys: ["?"], label: "打开或关闭快捷键说明" },
            ],
        },
    ]
}

export type WorkspaceHotkeyCheatSheetButtonProps = Readonly<{
    id?: string
    scope?: WorkspaceHotkeyCheatSheetScope
    open?: boolean
    onOpenChange?: (open: boolean) => void
    className?: string
}>

/** 紧凑快捷键图标按钮与速查弹窗（按 `?` 亦可直接开关，仅对当前激活页签生效）。 */
export function WorkspaceHotkeyCheatSheetButton({
    id = "workspace-hotkey-cheatsheet-trigger",
    scope = "queue",
    open: controlledOpen,
    onOpenChange,
    className,
}: WorkspaceHotkeyCheatSheetButtonProps) {
    const [uncontrolledOpen, setUncontrolledOpen] = useState(false)
    const open = controlledOpen ?? uncontrolledOpen
    const setOpen = (next: boolean) => {
        if (controlledOpen === undefined) setUncontrolledOpen(next)
        onOpenChange?.(next)
    }

    useEffect(() => {
        return subscribeWorkspaceHotkeyCheatSheetToggle(() => {
            if (controlledOpen === undefined) {
                setUncontrolledOpen((current) => !current)
            } else {
                onOpenChange?.(!controlledOpen)
            }
        })
    }, [controlledOpen, onOpenChange])

    const groups = hotkeyGroupsForScope(scope)

    return (
        <>
            <span className={cn("inline-flex shrink-0", className)}>
                <IconActionButton
                    id={id}
                    testId="workspace-hotkey-cheatsheet-trigger"
                    label="键盘快捷键 (?)"
                    onClick={() => setOpen(!open)}
                >
                    <KeyboardIcon aria-hidden="true" />
                </IconActionButton>
            </span>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    size="sm"
                    closeButtonId={`${id}-close`}
                    data-slot="workspace-hotkey-cheatsheet"
                >
                    <DialogHeader>
                        <DialogTitle>键盘快捷键</DialogTitle>
                        <DialogDescription>
                            仅在当前激活工作页生效；处于输入框、中文输入法或弹窗内时自动让渡。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-4">
                        {groups.map((group) => (
                            <section
                                key={group.heading}
                                className="space-y-1.5"
                            >
                                <h3 className="text-xs font-medium text-muted-foreground">
                                    {group.heading}
                                </h3>
                                <ul className="divide-y divide-border/60 border-y border-border/60">
                                    {group.items.map((item) => (
                                        <li
                                            key={`${group.heading}-${item.label}`}
                                            className="flex items-center justify-between gap-3 py-2 text-body-compact"
                                        >
                                            <span className="text-foreground">
                                                {item.label}
                                            </span>
                                            <span className="flex shrink-0 items-center gap-1">
                                                {item.keys.map((key) => (
                                                    <kbd
                                                        key={key}
                                                        className="inline-flex min-w-6 items-center justify-center rounded border border-border/80 bg-muted/60 px-1.5 py-0.5 font-mono text-xs font-medium text-foreground"
                                                    >
                                                        {key}
                                                    </kbd>
                                                ))}
                                            </span>
                                        </li>
                                    ))}
                                </ul>
                            </section>
                        ))}
                    </div>
                </DialogContent>
            </Dialog>
        </>
    )
}
