"use client"

import {
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import { z } from "zod"
import type { AccountProfile } from "@/features/auth/api"
import { apiEventStream } from "@/lib/api/event-stream"
import { createRefreshQueue, refreshFrameSchema } from "../lib/refresh"
import type { InboxCursor } from "../api"
import { getInbox, readAllNotifications, readNotification } from "../api"

const snapshotSchema = refreshFrameSchema.extend({
    unread_count: z.number().int().nonnegative(),
    todo_count: z.number().int().nonnegative().nullable(),
})
export type NotificationSnapshot = z.infer<typeof snapshotSchema>

export const notificationKeys = {
    all: ["notifications"] as const,
    inbox: (userId?: string) => ["notifications", userId, "inbox"] as const,
    feed: (profile?: AccountProfile) =>
        [
            "notifications",
            profile?.userid,
            "feed",
            profile?.policy_version,
            profile?.organization_version,
        ] as const,
}

/** 顶栏建立唯一订阅；其他组件只观察同一 Query 缓存。 */
export function useNotificationFeed(profile?: AccountProfile, connect = false) {
    const client = useQueryClient()
    const key = notificationKeys.feed(profile)
    return useQuery({
        queryKey: key,
        queryFn: async ({ signal }) => {
            let latest: NotificationSnapshot | undefined
            const refresh = createRefreshQueue(
                client,
                notificationKeys.inbox(profile?.userid),
                signal,
            )
            try {
                await apiEventStream(
                    "/account/notifications/events",
                    signal,
                    (event, raw) => {
                        if (signal.aborted) return
                        if (event === "snapshot") {
                            latest = snapshotSchema.parse(JSON.parse(raw))
                            client.setQueryData(key, latest)
                            refresh.enqueue(latest.topics)
                        } else if (event === "refresh") {
                            refresh.enqueue(
                                refreshFrameSchema.parse(JSON.parse(raw))
                                    .topics,
                            )
                        }
                    },
                )
            } finally {
                refresh.close()
            }
            if (!latest) throw new Error("实时消息暂不可用，请稍后重试")
            return latest
        },
        enabled: connect && Boolean(profile),
        meta: { feedback: { subject: "实时通知和待办数量" } },
        refetchInterval: connect
            ? (query) => (query.state.status === "error" ? 10_000 : 1_000)
            : false,
        refetchIntervalInBackground: true,
        refetchOnWindowFocus: false,
        refetchOnReconnect: true,
        retry: false,
        staleTime: Infinity,
        gcTime: 0,
    })
}

export function useInbox(userId?: string, open = false) {
    return useInfiniteQuery({
        queryKey: notificationKeys.inbox(userId),
        queryFn: ({ pageParam }) => getInbox(pageParam),
        initialPageParam: undefined as InboxCursor | undefined,
        getNextPageParam: (page) => {
            const last = page.items.at(-1)
            return page.has_more && last
                ? { before_created_at: last.created_at, before_id: last.id }
                : undefined
        },
        enabled: Boolean(userId) && open,
        staleTime: 0,
    })
}

export function useReadNotification(userId?: string) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: readNotification,
        onSuccess: () =>
            client.invalidateQueries({
                queryKey: notificationKeys.inbox(userId),
            }),
    })
}

export function useReadAllNotifications(profile?: AccountProfile) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: readAllNotifications,
        onSuccess: () => {
            if (profile) {
                client.setQueryData<NotificationSnapshot>(
                    notificationKeys.feed(profile),
                    (current) =>
                        current ? { ...current, unread_count: 0 } : current,
                )
            }
            return client.invalidateQueries({
                queryKey: notificationKeys.inbox(profile?.userid),
            })
        },
    })
}
