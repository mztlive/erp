"use client"

import { useState } from "react"
import { InboxIcon } from "lucide-react"
import { useRouter } from "next/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    useInbox,
    useNotificationFeed,
    useReadNotification,
} from "../hooks/queries"
import type { NotificationMessage } from "../api"

export function InboxPopover() {
    const [open, setOpen] = useState(false)
    const profile = useAccountProfileQuery().data
    const feed = useNotificationFeed(profile, true)
    const inbox = useInbox(profile?.userid, open)
    const read = useReadNotification(profile?.userid)
    const router = useRouter()
    const unread = feed.isError ? undefined : feed.data?.unread_count

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
        <Popover
            open={open}
            onOpenChange={(nextOpen, details) => {
                if (
                    !nextOpen &&
                    (details.reason === "focus-out" ||
                        details.reason === "outside-press")
                ) {
                    details.cancel()
                    return
                }
                setOpen(nextOpen)
            }}
        >
            <PopoverTrigger
                render={
                    <Button
                        id="workspace-inbox-trigger"
                        variant="ghost"
                        size="icon"
                        className="relative"
                        aria-label={
                            unread ? `收件箱，${unread} 条未读消息` : "收件箱"
                        }
                    />
                }
            >
                <InboxIcon className="size-5" />
                {unread ? (
                    <span className="absolute -right-1 -top-1 rounded-full bg-destructive px-1 text-xs leading-4 text-destructive-foreground">
                        {unread > 99 ? "99+" : unread}
                    </span>
                ) : null}
                {feed.isError ? (
                    <span className="absolute right-0 top-0 size-2 rounded-full bg-destructive" />
                ) : null}
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-96 max-w-[calc(100vw-2rem)] gap-3"
            >
                <div className="flex items-center justify-between gap-3">
                    <PopoverTitle>收件箱</PopoverTitle>
                    <span className="text-xs text-muted-foreground">
                        {inbox.data
                            ? `${inbox.data.pages[0].unread_count} 条未读`
                            : ""}
                    </span>
                </div>
                {feed.isError ? (
                    <p role="status" className="text-xs text-muted-foreground">
                        实时更新暂不可用，正在重新连接。
                    </p>
                ) : null}
                {read.isError ? (
                    <p role="alert" className="text-sm text-destructive">
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
                                    disabled={read.isPending}
                                    onClick={() => void openMessage(message)}
                                    className={cn(
                                        "w-full rounded-xl p-3 text-left transition-colors hover:bg-muted focus-visible:outline-ring disabled:opacity-60",
                                        message.read_at == null &&
                                            "bg-muted/60",
                                    )}
                                >
                                    <div className="flex items-start gap-2">
                                        <span className="flex-1 text-sm font-medium">
                                            {message.title}
                                        </span>
                                        {message.read_at == null ? (
                                            <span
                                                className="mt-1.5 size-1.5 shrink-0 rounded-full bg-destructive"
                                                aria-label="未读"
                                            />
                                        ) : null}
                                    </div>
                                    <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-muted-foreground">
                                        {message.body}
                                    </p>
                                    <time className="mt-2 block text-xs text-muted-foreground">
                                        {new Date(
                                            message.created_at * 1000,
                                        ).toLocaleString("zh-CN", {
                                            month: "2-digit",
                                            day: "2-digit",
                                            hour: "2-digit",
                                            minute: "2-digit",
                                        })}
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
