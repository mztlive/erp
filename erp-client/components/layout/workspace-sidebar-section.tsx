"use client"

import { useEffect, useState, type ReactNode } from "react"
import { ChevronDownIcon, ListChecksIcon } from "lucide-react"
import {
    SidebarGroup,
    SidebarGroupLabel,
    SidebarMenu,
    SidebarMenuItem,
    SidebarMenuButton,
} from "@/components/ui/sidebar"
import {
    Collapsible,
    CollapsibleTrigger,
    CollapsibleContent,
} from "@/components/ui/collapsible"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

const COLLAPSED_STORAGE_KEY = "erp.sidebar.collapsed-groups.v1"

function readCollapsedGroups(): Set<string> {
    if (typeof window === "undefined") return new Set()
    try {
        const raw = window.localStorage.getItem(COLLAPSED_STORAGE_KEY)
        if (!raw) return new Set()
        const parsed = JSON.parse(raw)
        return Array.isArray(parsed)
            ? new Set(parsed.filter((v): v is string => typeof v === "string"))
            : new Set()
    } catch {
        return new Set()
    }
}

function writeCollapsedGroup(label: string, collapsed: boolean): void {
    if (typeof window === "undefined") return
    try {
        const current = readCollapsedGroups()
        if (collapsed) current.add(label)
        else current.delete(label)
        window.localStorage.setItem(
            COLLAPSED_STORAGE_KEY,
            JSON.stringify([...current]),
        )
    } catch {
        // 隐私模式下忽略本地偏好写入失败。
    }
}

/**
 * 飞书风格可折叠侧栏分组：
 * - 「待办任务」保留一级聚合入口；
 * - 多项业务分组支持点击分组标题折叠/展开，并记住用户折叠偏好；
 * - 当前路由命中组内某项时自动展开，保证始终定位可见。
 */
export function WorkspaceSidebarSection({
    label,
    index,
    active,
    collapsible = true,
    count,
    children,
}: {
    label: string
    index: number
    active: boolean
    collapsible?: boolean
    count?: number
    children: ReactNode
}) {
    const [open, setOpen] = useState(true)

    useEffect(() => {
        if (label === "待办任务" || !collapsible) return
        if (active) {
            setOpen(true)
            return
        }
        setOpen(!readCollapsedGroups().has(label))
    }, [active, collapsible, label])

    useEffect(() => {
        if (active) setOpen(true)
    }, [active])

    const handleOpenChange = (next: boolean) => {
        setOpen(next)
        if (label !== "待办任务" && collapsible) {
            writeCollapsedGroup(label, !next)
        }
    }

    if (label !== "待办任务") {
        if (!collapsible) {
            return (
                <SidebarGroup className="gap-0.5 px-1 py-1">
                    <SidebarGroupLabel
                        className={index > 0 ? "mt-1.5" : undefined}
                    >
                        {label}
                    </SidebarGroupLabel>
                    {children}
                </SidebarGroup>
            )
        }
        return (
            <SidebarGroup className="gap-0.5 px-1 py-1">
                <Collapsible open={open} onOpenChange={handleOpenChange}>
                    <CollapsibleTrigger
                        id={`workspace-sidebar-group-${toAutomationIdSegment(label)}`}
                        type="button"
                        aria-label={`${open ? "收起" : "展开"}${label}分组`}
                        className={cn(
                            "group/sidebar-group-trigger flex h-6 w-full cursor-pointer items-center justify-between rounded-md px-2.5 text-tiny font-medium text-muted-foreground/85 transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring group-data-[collapsible=icon]:hidden",
                            index > 0 && "mt-1.5",
                        )}
                    >
                        <span className="truncate">{label}</span>
                        <span className="flex shrink-0 items-center gap-1">
                            {!open && count !== undefined && count > 0 ? (
                                <span className="num rounded-full bg-muted px-1.5 text-2xs font-medium leading-4 text-muted-foreground">
                                    {count}
                                </span>
                            ) : null}
                            <ChevronDownIcon
                                aria-hidden="true"
                                className={cn(
                                    "size-3 text-muted-foreground/70 transition-transform duration-200",
                                    !open && "-rotate-90",
                                )}
                            />
                        </span>
                    </CollapsibleTrigger>
                    <CollapsibleContent className="h-(--collapsible-panel-height) overflow-hidden transition-[height,opacity] duration-200 data-starting-style:h-0 data-starting-style:opacity-0 data-ending-style:h-0 data-ending-style:opacity-0 motion-reduce:transition-none group-data-[collapsible=icon]:block group-data-[collapsible=icon]:h-auto group-data-[collapsible=icon]:opacity-100">
                        {children}
                    </CollapsibleContent>
                </Collapsible>
            </SidebarGroup>
        )
    }

    return (
        <SidebarGroup className="px-1 py-1">
            <SidebarGroupLabel className="sr-only">待办任务</SidebarGroupLabel>
            <SidebarMenu>
                <SidebarMenuItem>
                    <Collapsible open={open} onOpenChange={setOpen}>
                        <CollapsibleTrigger
                            render={
                                <SidebarMenuButton
                                    id="workspace-sidebar-tasks-toggle"
                                    tooltip="待办任务"
                                />
                            }
                        >
                            <ListChecksIcon aria-hidden="true" />
                            <span>待办任务</span>
                            <span className="ml-auto flex shrink-0 items-center gap-1.5 group-data-[collapsible=icon]:hidden">
                                {count !== undefined && count > 0 && (
                                    <span
                                        className="num rounded-full bg-muted px-1.5 py-0 text-2xs font-medium leading-4.5 text-muted-foreground"
                                        title="待我处理与未读抄送"
                                    >
                                        {count}
                                    </span>
                                )}
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className={`size-3.5 text-muted-foreground transition-transform duration-200 ${open ? "rotate-180" : ""}`}
                                />
                            </span>
                        </CollapsibleTrigger>
                        <CollapsibleContent className="h-(--collapsible-panel-height) overflow-hidden transition-[height,opacity] duration-200 data-starting-style:h-0 data-starting-style:opacity-0 data-ending-style:h-0 data-ending-style:opacity-0 motion-reduce:transition-none group-data-[collapsible=icon]:hidden">
                            <div className="ml-3 border-l border-sidebar-border py-1 pl-2">
                                {children}
                            </div>
                        </CollapsibleContent>
                    </Collapsible>
                </SidebarMenuItem>
            </SidebarMenu>
        </SidebarGroup>
    )
}
