"use client"

import { useEffect, useState } from "react"
import { KeyboardIcon } from "lucide-react"

import { IconActionButton } from "@/components/business"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { subscribeWorkspaceHotkeyCheatSheetToggle } from "@/lib/workspace-hotkeys"
import { cn } from "@/lib/utils"

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
