"use client"

import * as React from "react"
import {
    useIsMutating,
    useMutation,
    useQueryClient,
} from "@tanstack/react-query"
import { apiPost, apiPut } from "@/lib/api"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { inquiryTaskCompletion } from "../lib/task-completion"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"

type SavedInquiryCommand = {
    path: string
    method: "POST" | "PUT"
    body: Record<string, unknown>
    recovery?: boolean
    uncertain?: boolean
}
export type InquiryCommandResolution = {
    path: string
    result: unknown
    replayed: boolean
    body: Record<string, unknown>
}

/** 在查询失效前通知当前页面，普通提交与原命令恢复共用同一成功出口。 */
export function useInquiryCommandResolution(
    userId: string,
    scope: string,
    onResolved: (resolution: InquiryCommandResolution) => void,
) {
    const key = inquiryStorageKey(userId, `${scope}:command`)
    React.useEffect(() => {
        const resolved = (event: Event) => {
            if (event instanceof CustomEvent && event.detail?.key === key)
                onResolved(event.detail.resolution)
        }
        window.addEventListener("inquiry-command-resolved", resolved)
        return () =>
            window.removeEventListener("inquiry-command-resolved", resolved)
    }, [key, onResolved])
}

/** 在发送前保存原命令；响应未知时跨刷新保留，恢复时只重发原载荷。 */
export function useInquiryCommand(userId: string, scope: string) {
    const client = useQueryClient()
    const key = inquiryStorageKey(userId, `${scope}:command`)
    const mutationKey = ["sales-inquiries", "command", userId, scope] as const
    const mutationFilters = { mutationKey, exact: true }
    const activeCommands = useIsMutating(mutationFilters)
    const [pending, setPending] = React.useState<SavedInquiryCommand | null>(
        null,
    )
    const [ready, setReady] = React.useState(false)
    const [resolution, setResolution] =
        React.useState<InquiryCommandResolution | null>(null)
    useInquiryCommandResolution(userId, scope, setResolution)
    React.useEffect(() => {
        const restore = () =>
            setPending(readInquiryLocalValue<SavedInquiryCommand>(key) ?? null)
        restore()
        setReady(true)
        window.addEventListener("inquiry-command-changed", restore)
        window.addEventListener("storage", restore)
        return () => {
            window.removeEventListener("inquiry-command-changed", restore)
            window.removeEventListener("storage", restore)
        }
    }, [key])
    const mutation = useMutation({
        mutationKey,
        mutationFn: async (command: SavedInquiryCommand) => {
            writeInquiryLocalValue(key, command)
            setPending(command)
            window.dispatchEvent(new Event("inquiry-command-changed"))
            return command.method === "PUT"
                ? apiPut<unknown>(command.path, command.body)
                : apiPost<unknown>(command.path, command.body)
        },
        meta: { suppressErrorToast: true },
        onSuccess: async (result, command) => {
            try {
                localStorage.removeItem(key)
            } catch {
                /* 业务成功；清理失败时仍可按原命令安全回放。 */
            }
            setPending(null)
            window.dispatchEvent(new Event("inquiry-command-changed"))
            const resolved = {
                path: command.path,
                result,
                replayed: command.recovery === true,
                body: command.body,
            }
            setResolution(resolved)
            window.dispatchEvent(
                new CustomEvent("inquiry-command-resolved", {
                    detail: { key, resolution: resolved },
                }),
            )
            const completion = inquiryTaskCompletion(scope, command)
            const endedTaskQuery = (queryKey: readonly unknown[]) =>
                completion != null &&
                ((queryKey[1] === "detail" &&
                    queryKey[2] === true &&
                    queryKey[3] === scope &&
                    queryKey[4] === completion.taskId) ||
                    (queryKey[1] === "internal-quotes" &&
                        queryKey[2] === scope &&
                        queryKey[3] === completion.taskId))
            if (completion) {
                await client.cancelQueries({
                    queryKey: ["sales-inquiries"],
                    predicate: (query) => endedTaskQuery(query.queryKey),
                })
                // 再次从已处理列表打开时必须读取只读结果，不能复用旧的可编辑任务。
                client.removeQueries({
                    queryKey: ["sales-inquiries"],
                    predicate: (query) => endedTaskQuery(query.queryKey),
                })
            }
            await Promise.all([
                client.invalidateQueries({
                    queryKey: ["sales-inquiries"],
                    predicate: (query) => !endedTaskQuery(query.queryKey),
                }),
                client.invalidateQueries({
                    queryKey: queryKeyRoots.workspaceHome,
                }),
                client.invalidateQueries({
                    queryKey: queryKeyRoots.salesOrders,
                }),
                client.invalidateQueries({
                    queryKey: queryKeyRoots.purchaseOrders,
                }),
            ])
        },
        onError: async (error, command) => {
            if (
                classifyFormalCommandError(error) === "failed" &&
                !command.recovery &&
                !command.uncertain
            ) {
                try {
                    localStorage.removeItem(key)
                } catch {
                    /* 保留原记录比更换命令安全。 */
                }
                setPending(null)
                window.dispatchEvent(new Event("inquiry-command-changed"))
                await client.invalidateQueries({
                    queryKey: ["sales-inquiries"],
                })
            } else {
                const uncertain = { ...command, uncertain: true }
                try {
                    writeInquiryLocalValue(key, uncertain)
                } catch {
                    /* 发送前已持久化原命令。 */
                }
                setPending(uncertain)
                window.dispatchEvent(new Event("inquiry-command-changed"))
            }
        },
    })
    // 本地提交记录由同一询价的多个组件共享，进行中状态必须按同一范围判断。
    const isPending = mutation.isPending || activeCommands > 0
    return {
        ...mutation,
        isPending,
        ready,
        resolution,
        unresolved: pending !== null && !isPending,
        async run<T>(
            path: string,
            body: Record<string, unknown>,
            method: "POST" | "PUT" = "POST",
            keyField: "command_key" | "idempotency_key" = "command_key",
            commandKey?: string,
        ): Promise<T> {
            if (!ready) throw new Error("正在恢复上次操作，请稍候。")
            if (client.isMutating(mutationFilters) > 0)
                throw new Error("当前操作正在提交，请稍候。")
            if (pending || readInquiryLocalValue(key))
                throw new Error("请先核对上次提交结果，再继续操作。")
            return (await mutation.mutateAsync({
                path,
                method,
                body: {
                    ...body,
                    [keyField]: commandKey ?? `inquiry:${crypto.randomUUID()}`,
                },
            })) as T
        },
        async recover<T>(): Promise<T | undefined> {
            if (client.isMutating(mutationFilters) > 0)
                throw new Error("当前操作正在提交，请稍候。")
            if (!pending) return undefined
            return (await mutation.mutateAsync({
                ...pending,
                recovery: true,
            })) as T
        },
    }
}
