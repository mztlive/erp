"use client"

import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronDownIcon, ListChecksIcon, LogOutIcon } from "lucide-react"

import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { logoutAndRedirect } from "@/components/providers/auth-session-provider"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"

function displayInitial(
    name: string | undefined,
    account: string | undefined,
): string {
    const source = (name || account || "用").trim()
    return source.slice(0, 1).toUpperCase() || "用"
}

export function WorkspaceAccountMenu() {
    const router = useRouter()
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()

    const canViewBackgroundJobs = hasPermission(
        profileQuery.data?.permissions,
        "background_job:list",
    )

    const displayName =
        profileQuery.data?.name || profileQuery.data?.account || "已登录"
    const accountLabel = profileQuery.data?.account || "后台账号"

    return (
        <div className="flex min-w-0 items-center gap-2">
            {!profileQuery.isPending && !canViewBackgroundJobs && (
                <div className="min-w-0">
                    <Button
                        id="workspace-topbar-file-tasks"
                        data-workspace-nav="account-data-transfers"
                        render={<Link href="/account/data-transfers" />}
                        variant="ghost"
                        size="icon-sm"
                        aria-label="我的文件任务"
                        title="我的文件任务"
                    >
                        <ListChecksIcon aria-hidden="true" />
                    </Button>
                </div>
            )}
            <div className="min-w-0">
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="workspace-topbar-account-trigger"
                        render={
                            <Button
                                variant="ghost"
                                className="h-9 max-w-56 gap-2 px-2"
                                aria-label="账号菜单"
                            />
                        }
                    >
                        <Avatar size="sm" className="size-7 rounded-md">
                            <AvatarFallback className="rounded-md bg-zinc-900 font-semibold text-zinc-100 text-xs dark:bg-zinc-100 dark:text-zinc-900">
                                {displayInitial(
                                    profileQuery.data?.name,
                                    profileQuery.data?.account,
                                )}
                            </AvatarFallback>
                        </Avatar>
                        <span className="hidden min-w-0 text-left text-sm leading-tight sm:block">
                            <span className="block truncate font-medium">
                                {displayName}
                            </span>
                        </span>
                        <ChevronDownIcon
                            className="ml-0.5 size-3.5 text-muted-foreground"
                            aria-hidden="true"
                        />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        side="bottom"
                        align="end"
                        className="min-w-56"
                    >
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                <div className="flex flex-col gap-0.5">
                                    <span className="text-sm font-medium text-foreground">
                                        {displayName}
                                    </span>
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {accountLabel}
                                    </span>
                                </div>
                            </DropdownMenuLabel>
                        </DropdownMenuGroup>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                            id="workspace-topbar-home-settings"
                            onClick={() => router.push("/account/workbench")}
                        >
                            个人首页设置
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-topbar-my-transfers"
                            onClick={() =>
                                router.push("/account/data-transfers")
                            }
                        >
                            我的文件任务
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-topbar-account-logout"
                            variant="destructive"
                            onClick={() =>
                                logoutAndRedirect(router, queryClient)
                            }
                        >
                            <LogOutIcon aria-hidden="true" />
                            退出登录
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>
        </div>
    )
}
