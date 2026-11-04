import { apiGet, apiPost } from "@/lib/api"

export interface NotificationMessage {
    id: string
    title: string
    body: string
    href: string | null
    created_at: number
    read_at: number | null
}

export interface InboxPage {
    items: NotificationMessage[]
    unread_count: number
    has_more: boolean
}

export interface InboxCursor {
    before_created_at: number
    before_id: string
}
export const getInbox = (cursor?: InboxCursor) =>
    apiGet<InboxPage>(
        "/account/notifications",
        cursor ? { ...cursor } : undefined,
    )
export const readNotification = (id: string) =>
    apiPost<void>(`/account/notifications/${encodeURIComponent(id)}/read`)
