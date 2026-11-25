"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { AppWindowIcon, ClipboardCheckIcon, SearchIcon } from "lucide-react"

import {
    Command,
    CommandDialog,
    CommandEmpty,
    CommandGroup,
    CommandInput,
    CommandItem,
    CommandList,
    CommandSeparator,
} from "@/components/ui/command"
import { Kbd } from "@/components/ui/kbd"
import { SidebarMenuButton } from "@/components/ui/sidebar"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
} from "@/lib/workspace-registry"
import { useWorkspaceShellBridge } from "@/lib/workspace-shell-bridge"
import {
    isWorkspaceQuickSearchShortcut,
    workspaceHost,
} from "@/lib/workspace-tabs"
import { useWorkspaceOpenTabs } from "./workspace-tabs"

const QUICK_SEARCH_EVENT = "erp-workspace-quick-search"
const TASK_SEARCH_DEBOUNCE_MS = 250
const TASK_SEARCH_LIMIT = 6

/** 允许宿主与内嵌工作页共同唤起全局快捷跳转面板。 */
export function dispatchWorkspaceQuickSearch(): void {
    if (typeof window === "undefined") return
    window.dispatchEvent(new Event(QUICK_SEARCH_EVENT))
}

export function WorkspaceQuickSearch() {
    const [open, setOpen] = React.useState(false)
    const [search, setSearch] = React.useState("")
    const [debouncedQuery, setDebouncedQuery] = React.useState("")
    const router = useRouter()
    const { profile: profileQuery, searchQuickTasks } =
        useWorkspaceShellBridge()
    const permissions = profileQuery.data?.permissions
    const canListWorkItems = hasPermission(permissions, "work_item:list")
    const { tabs: openTabs, selectTab } = useWorkspaceOpenTabs()

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
        if (!open) {
            setSearch("")
            setDebouncedQuery("")
            return
        }
        const trimmed = search.trim().slice(0, 128)
        if (!trimmed) {
            setDebouncedQuery("")
            return
        }
        const timer = window.setTimeout(() => {
            setDebouncedQuery(trimmed)
        }, TASK_SEARCH_DEBOUNCE_MS)
        return () => window.clearTimeout(timer)
    }, [open, search])

    const taskSearchQuery = useQuery({
        queryKey: [
            "workspace-quick-search-tasks",
            profileQuery.data?.userid,
            profileQuery.data?.policy_version,
            profileQuery.data?.organization_version,
            debouncedQuery,
        ],
        queryFn: () =>
            searchQuickTasks({
                query: debouncedQuery,
                limit: TASK_SEARCH_LIMIT,
            }),
        enabled: open && canListWorkItems && debouncedQuery.length > 0,
        staleTime: 15_000,
        retry: false,
    })

    const matchedTasks = taskSearchQuery.data ?? []
    const showTaskGroup =
        canListWorkItems &&
        debouncedQuery.length > 0 &&
        (taskSearchQuery.isFetching || matchedTasks.length > 0)

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (isWorkspaceQuickSearchShortcut(event)) {
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

    const handleSelectOpenTab = (tabId: string) => {
        setOpen(false)
        selectTab(tabId)
    }

    return (
        <>
            <SidebarMenuButton
                id="workspace-quick-search-trigger"
                type="button"
                tooltip="快速跳转页签、任务与菜单 (⌘K)"
                aria-label="快速跳转"
                className="h-control-sm border-0 bg-shell-hover text-muted-foreground/85 shadow-none hover:bg-shell-strong hover:text-foreground"
                onClick={() => setOpen(true)}
            >
                <SearchIcon
                    className="size-3.5 shrink-0 text-muted-foreground/70"
                    aria-hidden="true"
                />
                <span className="flex-1 truncate text-xs">快速跳转</span>
                <Kbd aria-hidden="true" className="ml-auto">
                    ⌘K
                </Kbd>
            </SidebarMenuButton>
            <CommandDialog
                open={open}
                onOpenChange={setOpen}
                title="快速跳转"
                description="搜索已打开页签、任务单号、往来单位或功能模块"
            >
                <Command>
                    <CommandInput
                        id="workspace-quick-search-input"
                        value={search}
                        onValueChange={setSearch}
                        placeholder="搜索已打开页签、单号、往来单位或菜单…"
                    />
                    <CommandList className="max-h-96 p-1">
                        <CommandEmpty>
                            {taskSearchQuery.isFetching
                                ? "正在检索任务与往来单位…"
                                : "没有找到匹配的页签、任务或菜单"}
                        </CommandEmpty>

                        {openTabs.length > 0 ? (
                            <>
                                <CommandGroup heading="已打开页签">
                                    {openTabs.map((tab) => (
                                        <CommandItem
                                            key={`open-tab-${tab.id}`}
                                            id={`workspace-quick-search-tab-${toAutomationIdSegment(tab.id)}`}
                                            value={`已打开页签 ${tab.title} ${tab.href}`}
                                            onSelect={() =>
                                                handleSelectOpenTab(tab.id)
                                            }
                                        >
                                            <AppWindowIcon aria-hidden="true" />
                                            <span className="min-w-0 flex-1 truncate">
                                                {tab.title}
                                            </span>
                                            {tab.dirty ? (
                                                <span className="shrink-0 text-tiny text-warning-soft-foreground">
                                                    未保存
                                                </span>
                                            ) : null}
                                            {tab.active ? (
                                                <span className="shrink-0 rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                                                    当前
                                                </span>
                                            ) : null}
                                        </CommandItem>
                                    ))}
                                </CommandGroup>
                                <CommandSeparator />
                            </>
                        ) : null}

                        {showTaskGroup ? (
                            <>
                                <CommandGroup heading="任务与往来单位直达">
                                    {taskSearchQuery.isFetching &&
                                    matchedTasks.length === 0 ? (
                                        <div
                                            role="status"
                                            className="px-2.5 py-2 text-xs text-muted-foreground"
                                        >
                                            正在检索匹配的任务与往来单位…
                                        </div>
                                    ) : (
                                        matchedTasks.map((task) => {
                                            const href =
                                                buildWorkspaceDetailHref(
                                                    "tasks",
                                                    task.id,
                                                    {
                                                        returnTo:
                                                            "/workspace/tasks",
                                                    },
                                                )
                                            const matchHints = (
                                                task.searchMatches ?? []
                                            ).join(" ")
                                            return (
                                                <CommandItem
                                                    key={`task-${task.id}`}
                                                    id={`workspace-quick-search-task-${toAutomationIdSegment(task.id)}`}
                                                    value={`${search} ${debouncedQuery} ${task.primary} ${task.secondary ?? ""} ${task.typeLabel} ${matchHints}`}
                                                    onSelect={() =>
                                                        handleSelect(href)
                                                    }
                                                >
                                                    <ClipboardCheckIcon aria-hidden="true" />
                                                    <div className="flex min-w-0 flex-1 flex-col">
                                                        <span className="truncate text-body-compact text-foreground">
                                                            {task.primary}
                                                        </span>
                                                        {task.secondary ? (
                                                            <span className="num truncate text-xs text-muted-foreground">
                                                                {task.secondary}
                                                            </span>
                                                        ) : null}
                                                    </div>
                                                    <span className="shrink-0 rounded-sm bg-muted/75 px-1.5 py-0.5 text-xs text-muted-foreground">
                                                        {task.typeLabel}
                                                    </span>
                                                </CommandItem>
                                            )
                                        })
                                    )}
                                </CommandGroup>
                                <CommandSeparator />
                            </>
                        ) : null}

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
