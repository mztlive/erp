"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    authorizeJdVopAccount,
    fetchJdVopAuthorization,
} from "../api/jd-vop-authorization"
import type { AuthorizeJdVopAccount } from "../types/jd-vop-authorization"
import { supplierConnectionKeys } from "./queries"

const POLL_WINDOW_MS = 2 * 60 * 1000
const root = ["supplier-api-connections"] as const
export const jdVopAuthorizationKeys = {
    status: (connectionId: string, operationId: string | null = null) =>
        [...root, "jd-vop-authorization", connectionId, operationId] as const,
    authorize: (connectionId: string) =>
        [...root, "jd-vop-authorize", connectionId] as const,
}

export function useJdVopAuthorizationQuery(
    connectionId: string,
    enabled: boolean,
) {
    const client = useQueryClient()
    const refreshedAuthorization = useRef<string | null>(null)
    const [pollUntil, setPollUntil] = useState(
        () => Date.now() + POLL_WINDOW_MS,
    )
    const [pendingOperationId, setPendingOperationId] = useState<string | null>(
        null,
    )
    const query = useQuery({
        queryKey: jdVopAuthorizationKeys.status(
            connectionId,
            pendingOperationId,
        ),
        queryFn: () =>
            fetchJdVopAuthorization(
                connectionId,
                pendingOperationId ?? undefined,
            ),
        enabled,
        retry: false,
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        refetchInterval: (current) => {
            const waiting = ["authorizing", "refreshing", "unknown"].includes(
                current.state.data?.state ?? "",
            )
            const unconfirmed =
                pendingOperationId !== null &&
                current.state.data?.operation_id !== pendingOperationId
            return Date.now() < pollUntil && (waiting || unconfirmed)
                ? 2000
                : false
        },
    })
    const restartPolling = useCallback((operationId?: string | null) => {
        setPollUntil(Date.now() + POLL_WINDOW_MS)
        if (operationId !== undefined) setPendingOperationId(operationId)
    }, [])
    useEffect(() => {
        if (
            pendingOperationId &&
            query.data?.operation_id === pendingOperationId &&
            ["completed", "rejected", "superseded"].includes(
                query.data.operation_state ?? "",
            )
        ) {
            client.setQueryData(
                jdVopAuthorizationKeys.status(connectionId),
                query.data,
            )
            setPendingOperationId(null)
        }
    }, [client, connectionId, pendingOperationId, query.data])
    const unconfirmed =
        pendingOperationId !== null &&
        query.data?.operation_id !== pendingOperationId
    const authorized =
        query.data?.state === "authorized"
            ? `${connectionId}:${query.data.version}:${query.data.operation_id}`
            : null
    useEffect(() => {
        if (!authorized || refreshedAuthorization.current === authorized) return
        refreshedAuthorization.current = authorized
        void client.invalidateQueries({
            queryKey: supplierConnectionKeys.center(connectionId),
        })
        void client.invalidateQueries({
            queryKey: [...supplierConnectionKeys.all, "list"],
        })
    }, [authorized, client, connectionId])
    return { ...query, restartPolling, pollUntil, unconfirmed }
}

/** mutation 变量始终为空；密码和私钥只由本次请求的短生命周期引用持有。 */
export function useAuthorizeJdVopAccount(connectionId: string) {
    const client = useQueryClient()
    const payload = useRef<AuthorizeJdVopAccount | null>(null)
    const inFlight = useRef(false)
    const mutation = useMutation({
        mutationKey: jdVopAuthorizationKeys.authorize(connectionId),
        gcTime: 0,
        retry: false,
        networkMode: "always",
        meta: { suppressErrorToast: true },
        mutationFn: async () => {
            if (!payload.current)
                throw new Error("本次授权资料已清空，请重新填写。")
            return authorizeJdVopAccount(connectionId, payload.current)
        },
        onSuccess: async (status) => {
            client.setQueryData(
                jdVopAuthorizationKeys.status(connectionId),
                status,
            )
            await client.invalidateQueries({ queryKey: root })
        },
    })
    const resetMutation = mutation.reset
    const clearSecrets = useCallback(() => {
        if (payload.current) {
            payload.current.password = ""
            payload.current.private_key = ""
            payload.current.username = ""
            payload.current.redirect_uri = ""
            payload.current.evidence_reference = undefined
        }
        payload.current = null
        resetMutation()
        const cache = client.getMutationCache()
        for (const record of cache.findAll({
            mutationKey: jdVopAuthorizationKeys.authorize(connectionId),
            exact: true,
        }))
            cache.remove(record)
    }, [client, connectionId, resetMutation])
    useEffect(() => clearSecrets, [clearSecrets])
    const submit = async (input: AuthorizeJdVopAccount) => {
        if (inFlight.current)
            throw new Error("账号授权正在提交，请等待原结果。")
        inFlight.current = true
        payload.current = input
        try {
            return await mutation.mutateAsync()
        } finally {
            inFlight.current = false
            input.password = ""
            input.private_key = ""
            clearSecrets()
        }
    }
    return { submit, clearSecrets, isPending: mutation.isPending }
}
