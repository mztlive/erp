"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import {
    Command,
    CommandDialog,
    CommandEmpty,
    CommandGroup,
    CommandInput,
    CommandItem,
    CommandList,
} from "@/components/ui/command"
import { Kbd } from "@/components/ui/kbd"
import { SidebarMenuButton } from "@/components/ui/sidebar"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter } from "@/lib/navigation"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
} from "@/lib/workspace-registry"
import { workspaceHost } from "@/lib/workspace-tabs"

const QUICK_SEARCH_EVENT = "erp-workspace-quick-search"

/** 允许宿主与内嵌工作页共同唤起全局快捷跳转面板。 */
export function dispatchWorkspaceQuickSearch(): void {
    if (typeof window === "undefined") return
    window.dispatchEvent(new Event(QUICK_SEARCH_EVENT))
}

export function WorkspaceQuickSearch() {
    const [open, setOpen] = React.useState(false)
    const router = useRouter()
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions

    const navGroups = React.useMemo(
        () =>
            filterNavGroupsByPermissions(WORKSPACE_NAV_GROUPS, permissions)
                .map((group) => ({
                    ...group,
                    items: group.items.filter(
                        (item) =>
                            item.badge !== "copy-unread-count" ||
                            profileQuery.data?.account_kind === "admin",
                    ),
                }))
                .filter((group) => group.items.length > 0),
        [permissions, profileQuery.data?.account_kind],
    )

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.altKey ||
                event.shiftKey
            )
                return
            if (
                event.key.toLowerCase() === "k" &&
                (event.metaKey || event.ctrlKey)
            ) {
                event.preventDefault()
                setOpen((current) => !current)
            }
        }
        const onEvent = () => setOpen(true)
        window.addEventListener("keydown", onKeyDown)
        window.addEventListener(QUICK_SEARCH_EVENT, onEvent)
        return () => {
            window.removeEventListener("keydown", onKeyDown)
            window.removeEventListener(QUICK_SEARCH_EVENT, onEvent)
        }
    }, [])

    const handleSelect = (href: string) => {
        setOpen(false)
        const host = workspaceHost()
        if (host) {
            host.navigate(null, href)
        } else {
            router.push(href)
        }
    }

    return (
        <>
            <SidebarMenuButton
                id="workspace-quick-search-trigger"
                type="button"
                tooltip="快速跳转菜单 (⌘K)"
                aria-label="快速跳转菜单"
                className="h-8 border border-border/80 bg-muted/55 text-muted-foreground shadow-none hover:border-primary/45 hover:bg-card hover:text-foreground group-data-[collapsible=icon]:border-transparent group-data-[collapsible=icon]:bg-transparent"
                onClick={() => setOpen(true)}
            >
                <SearchIcon className="size-3.5 shrink-0" aria-hidden="true" />
                <span className="flex-1 truncate text-xs group-data-[collapsible=icon]:hidden">
                    快速跳转…
                </span>
                <Kbd className="ml-auto h-4.5 min-w-0 px-1 text-2xs group-data-[collapsible=icon]:hidden">
                    ⌘K
                </Kbd>
            </SidebarMenuButton>
            <CommandDialog
                open={open}
                onOpenChange={setOpen}
                title="快速跳转"
                description="输入模块名称快速打开工作页"
            >
                <Command>
                    <CommandInput
                        id="workspace-quick-search-input"
                        placeholder="搜索菜单或功能模块…"
                    />
                    <CommandList className="max-h-80 p-1">
                        <CommandEmpty>没有找到匹配的菜单</CommandEmpty>
                        {navGroups.map((group) => (
                            <CommandGroup
                                key={group.label}
                                heading={group.label}
                            >
                                {group.items.map((item) => {
                                    const Icon = item.icon
                                    return (
                                        <CommandItem
                                            key={`${group.label}-${item.href}`}
                                            id={`workspace-quick-search-item-${toAutomationIdSegment(item.href)}`}
                                            value={`${group.label} ${item.label}`}
                                            onSelect={() =>
                                                handleSelect(item.href)
                                            }
                                        >
                                            <Icon aria-hidden="true" />
                                            <span>{item.label}</span>
                                        </CommandItem>
                                    )
                                })}
                            </CommandGroup>
                        ))}
                    </CommandList>
                </Command>
            </CommandDialog>
        </>
    )
}
