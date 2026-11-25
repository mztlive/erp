"use client"

import { useState } from "react"
import { InboxIcon } from "lucide-react"
import { useRouter } from "@/lib/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { cn } from "@/lib/utils"
import {
    useInbox,
    useNotificationFeed,
    useReadAllNotifications,
    useReadNotification,
} from "../hooks/queries"
import type { NotificationMessage } from "../api"

export function InboxPopover() {
    const [open, setOpen] = useState(false)
    const profile = useAccountProfileQuery().data
    const feed = useNotificationFeed(profile, true)
    const inbox = useInbox(profile?.userid, open)
    const read = useReadNotification(profile?.userid)
    const readAll = useReadAllNotifications(profile)
    const router = useRouter()
    const unread = feed.isError ? undefined : feed.data?.unread_count
    const inboxUnreadCount = inbox.data?.pages[0]?.unread_count ?? unread ?? 0

    async function openMessage(message: NotificationMessage) {
        if (message.read_at == null) {
            try {
                await read.mutateAsync(message.id)
            } catch {
                return
            }
        }
        if (
            message.href?.startsWith("/") &&
            !message.href.startsWith("//") &&
            !message.href.includes("\\")
        ) {
            setOpen(false)
            router.push(message.href)
        }
    }

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger
                render={
                    <Button
                        id="workspace-inbox-trigger"
                        variant="ghost"
                        size="icon-sm"
                        className="relative text-muted-foreground hover:text-foreground"
                        aria-label={
                            unread ? `收件箱，${unread} 条未读消息` : "收件箱"
                        }
                        title={unread ? `收件箱（${unread} 条未读）` : "收件箱"}
                    />
                }
            >
                <InboxIcon className="size-4" aria-hidden="true" />
                {unread ? (
                    <span className="num pointer-events-none absolute top-0.5 right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-destructive px-1 text-tiny font-semibold text-destructive-foreground ring-2 ring-card">
                        {unread > 99 ? "99+" : unread}
                    </span>
                ) : null}
                {feed.isError ? (
                    <span className="pointer-events-none absolute top-1.5 right-1.5 size-2 rounded-full bg-destructive ring-2 ring-card" />
                ) : null}
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-96 max-w-[calc(100vw-2rem)] gap-3"
            >
                <div className="flex items-center justify-between gap-2">
                    <div className="flex min-w-0 items-baseline gap-2">
                        <PopoverTitle>收件箱</PopoverTitle>
                        <span className="num text-xs text-muted-foreground">
                            {inbox.data
                                ? `${inbox.data.pages[0].unread_count} 条未读`
                                : ""}
                        </span>
                    </div>
                    {inboxUnreadCount > 0 ? (
                        <Button
                            id="workspace-inbox-read-all"
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={readAll.isPending || read.isPending}
                            onClick={() => readAll.mutate()}
                            className="shrink-0 text-xs text-primary hover:text-primary"
                        >
                            {readAll.isPending ? "正在标记…" : "全部标为已读"}
                        </Button>
                    ) : null}
                </div>
                {feed.isError ? (
                    <p role="status" className="text-xs text-muted-foreground">
                        实时更新暂不可用，正在重新连接。
                    </p>
                ) : null}
                {read.isError || readAll.isError ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        消息未能标记为已读，请重试。
                    </p>
                ) : null}
                {inbox.isError ? (
                    <div role="alert" className="space-y-2">
                        <p>消息加载失败，请重试。</p>
                        <Button
                            id="workspace-inbox-retry"
                            variant="outline"
                            size="sm"
                            onClick={() => void inbox.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : inbox.isPending ? (
                    <p className="py-8 text-center text-muted-foreground">
                        正在加载消息…
                    </p>
                ) : inbox.data.pages[0].items.length === 0 ? (
                    <p className="py-8 text-center text-muted-foreground">
                        暂无消息
                    </p>
                ) : (
                    <div
                        className="max-h-96 space-y-1 overflow-y-auto"
                        aria-label="消息列表"
                    >
                        {inbox.data.pages
                            .flatMap((page) => page.items)
                            .map((message) => (
                                <button
                                    key={message.id}
                                    id={`workspace-inbox-message-${toAutomationIdSegment(message.id)}`}
                                    type="button"
                                    disabled={
                                        read.isPending || readAll.isPending
                                    }
                                    onClick={() => void openMessage(message)}
                                    className={cn(
                                        "w-full rounded-lg p-3 text-left transition-colors hover:bg-muted focus-visible:outline-ring disabled:opacity-60",
                                        message.read_at == null &&
                                            "bg-muted/60",
                                    )}
                                >
                                    <div className="flex items-start gap-2">
                                        <span className="flex-1 text-body-compact font-medium">
                                            {message.title}
                                        </span>
                                        {message.read_at == null ? (
                                            <span
                                                className="mt-1.5 size-1.5 shrink-0 rounded-full bg-destructive"
                                                aria-label="未读"
                                            />
                                        ) : null}
                                    </div>
                                    <p className="mt-1 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                                        {message.body}
                                    </p>
                                    <time className="num mt-2 block text-xs text-muted-foreground">
                                        {formatDateTime(
                                            unixSecondsToIsoOrEmpty(
                                                message.created_at,
                                            ),
                                            "monthDayIntl",
                                        )}
                                    </time>
                                </button>
                            ))}
                        {inbox.hasNextPage ? (
                            <Button
                                id="workspace-inbox-load-more"
                                variant="ghost"
                                className="w-full"
                                disabled={inbox.isFetchingNextPage}
                                onClick={() => void inbox.fetchNextPage()}
                            >
                                {inbox.isFetchingNextPage
                                    ? "正在加载…"
                                    : "加载更多"}
                            </Button>
                        ) : null}
                    </div>
                )}
            </PopoverContent>
        </Popover>
    )
}
