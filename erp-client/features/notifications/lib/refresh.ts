import type { QueryClient, QueryKey } from "@tanstack/react-query"
import { z } from "zod"
import { queryKeyRoots as roots } from "@/lib/query-key-roots"

export const refreshTopicSchema = z.enum([
    "inbox",
    "tasks",
    "approvals",
    "copies",
    "inquiries",
    "selection",
    "jobs",
    "sales",
    "purchase",
    "finance",
    "fulfillment",
    "authorization",
])
export const refreshFrameSchema = z.object({
    topics: z.array(refreshTopicSchema),
})
type RefreshTopic = z.infer<typeof refreshTopicSchema>

const queries: Record<RefreshTopic, readonly QueryKey[]> = {
    inbox: [], // 个人收件箱键由订阅者传入，禁止使整个 notifications 根键失效。
    tasks: [roots.workspaceHome, roots.workItems, roots.salesInquiries],
    approvals: [
        roots.workspaceHome,
        roots.approvals,
        roots.salesOrders,
        roots.purchaseOrders,
        roots.paymentRequests,
    ],
    copies: [roots.approvalCopies],
    inquiries: [roots.salesInquiries, roots.workspaceHome],
    selection: [roots.salesSelection],
    jobs: [roots.backgroundJobs, roots.dataTransfers],
    sales: [roots.salesOrders, roots.paymentRequests, roots.workspaceHome],
    purchase: [
        roots.purchaseOrders,
        roots.salesOrders,
        roots.paymentRequests,
        roots.workspaceHome,
    ],
    finance: [
        roots.paymentRequests,
        roots.customerReceivables,
        roots.supplierPayables,
        roots.salesOrders,
        roots.purchaseOrders,
        roots.fulfillmentOperations,
        roots.workspaceHome,
    ],
    fulfillment: [
        roots.fulfillmentOperations,
        roots.salesOrders,
        roots.purchaseOrders,
        roots.workspaceHome,
    ],
    authorization: [], // 先刷新权限，再刷新各业务的授权计数。
}

/** 串行合并刷新批次；后续事件等待本批完成，避免持续取消导致查询饥饿。 */
export function createRefreshQueue(
    client: QueryClient,
    inboxKey: QueryKey,
    signal: AbortSignal,
) {
    const pending = new Set<RefreshTopic>()
    let running = false
    let closed = false
    async function refresh(queryKey: QueryKey) {
        // 事件之前开始的请求可能返回旧快照，包括尚无 data 的首次请求。
        await client.cancelQueries({ queryKey }, { silent: true })
        if (closed || signal.aborted) return
        await client.invalidateQueries({ queryKey })
    }
    async function flush() {
        if (running || closed || signal.aborted) return
        running = true
        try {
            while (pending.size && !closed && !signal.aborted) {
                const topics = new Set(pending)
                pending.clear()
                if (topics.has("authorization")) {
                    await refresh(roots.accountProfile)
                    for (const topic of refreshTopicSchema.options)
                        topics.add(topic)
                }
                if (closed || signal.aborted) return
                const keys = new Map<string, QueryKey>()
                if (topics.has("inbox"))
                    keys.set(JSON.stringify(inboxKey), inboxKey)
                for (const topic of topics) {
                    for (const key of queries[topic])
                        keys.set(JSON.stringify(key), key)
                }
                await Promise.allSettled([...keys.values()].map(refresh))
            }
        } finally {
            running = false
        }
    }
    return {
        enqueue(topics: readonly RefreshTopic[]) {
            if (closed || signal.aborted) return
            for (const topic of topics) pending.add(topic)
            void flush()
        },
        close() {
            closed = true
            pending.clear()
        },
    }
}
