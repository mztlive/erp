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

/** 待办四个页面使用可展开的一级入口，其他业务导航保持原分组。 */
export function WorkspaceSidebarSection({
    label,
    index,
    active,
    count,
    children,
}: {
    label: string
    index: number
    active: boolean
    count?: number
    children: ReactNode
}) {
    const [open, setOpen] = useState(true)
    useEffect(() => {
        if (active) setOpen(true)
    }, [active])
    if (label !== "待办任务")
        return (
            <SidebarGroup className="gap-1 px-1 py-1">
                <SidebarGroupLabel className={index > 0 ? "mt-2" : undefined}>
                    {label}
                </SidebarGroupLabel>
                {children}
            </SidebarGroup>
        )
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
                            {count !== undefined && count > 0 && (
                                <span
                                    className="num ml-auto text-xs text-muted-foreground"
                                    title="待我处理与未读抄送"
                                >
                                    {count}
                                </span>
                            )}
                            <ChevronDownIcon
                                aria-hidden="true"
                                className={`ml-auto transition-transform duration-200 ${open ? "rotate-180" : ""}`}
                            />
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
