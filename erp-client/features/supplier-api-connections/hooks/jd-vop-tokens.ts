"use client"
import { useCallback, useEffect, useRef } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { configureJdVopTokens } from "../api/jd-vop-authorization"
import type { ConfigureJdVopTokens } from "../types/jd-vop-authorization"
import { jdVopAuthorizationKeys } from "./jd-vop-authorization"
const root = ["supplier-api-connections"] as const

/** mutation 变量始终为空；令牌只由本次请求的短生命周期引用持有。 */
export function useConfigureJdVopTokens(connectionId: string) {
    const client = useQueryClient()
    const payload = useRef<ConfigureJdVopTokens | null>(null)
    const inFlight = useRef(false)
    const mutation = useMutation({
        mutationKey: ["jd-vop-configure-tokens", connectionId],
        gcTime: 0,
        retry: false,
        networkMode: "always",
        meta: { suppressErrorToast: true },
        mutationFn: async () => {
            if (!payload.current)
                throw new Error("本次令牌资料已清空，请重新填写。")
            return configureJdVopTokens(connectionId, payload.current)
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
            payload.current.access_token = ""
            payload.current.refresh_token = ""
        }
        payload.current = null
        resetMutation()
        const cache = client.getMutationCache()
        for (const record of cache.findAll({
            mutationKey: ["jd-vop-configure-tokens", connectionId],
            exact: true,
        }))
            cache.remove(record)
    }, [client, connectionId, resetMutation])
    useEffect(() => clearSecrets, [clearSecrets])
    const submit = async (input: ConfigureJdVopTokens) => {
        if (inFlight.current) throw new Error("令牌正在保存，请等待原结果。")
        inFlight.current = true
        payload.current = input
        try {
            return await mutation.mutateAsync()
        } finally {
            inFlight.current = false
            input.access_token = ""
            input.refresh_token = ""
            clearSecrets()
        }
    }
    return { submit, clearSecrets, isPending: mutation.isPending }
}
