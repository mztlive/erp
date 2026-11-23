"use client"

import * as React from "react"
import { Clock, RefreshCw } from "lucide-react"

import { Button } from "@/components/ui/button"
import { usePublicSelectionQuery } from "../queries"
import { requestFailure } from "../lib/public-flow"
import { PublicSelectionReceipt } from "../components/public-selection-receipt"
import { PublicSelectionEnded } from "../components/public-selection-ended"
import { SelectionForm } from "../components/public-selection-form"
import {
    PublicSelectionAccess,
    useSelectionAccess,
} from "../components/public-selection-access"

/** 读取公开选品事实，明确区分加载、结束、回执和可编辑状态。 */
export const PublicSelectionPage = ({ token }: { token: string }) => {
    const access = useSelectionAccess(token)
    const query = usePublicSelectionQuery(
        token,
        access.accessToken,
        access.ready,
    )
    const page = query.data
    const accessToken = access.accessToken
    const lockAccess = access.lock
    React.useEffect(() => {
        if (page?.kind === "LOCKED" && accessToken) lockAccess()
    }, [page?.kind, accessToken, lockAccess])

    if (query.isError && (!page || requestFailure(query.error) === "ended"))
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center">
                <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
                    <Clock className="h-7 w-7" />
                </div>
                <h1 className="text-xl font-bold text-foreground">
                    {requestFailure(query.error) === "ended"
                        ? "选品链接已失效"
                        : "暂时无法打开选品页"}
                </h1>
                <p className="my-3 text-sm leading-relaxed text-muted-foreground">
                    {requestFailure(query.error) === "ended"
                        ? "请联系销售核对当前链接。"
                        : "请检查网络连接后重试。"}
                </p>
                {requestFailure(query.error) !== "ended" && (
                    <Button
                        id="sales-selection-public-retry"
                        onClick={() => void query.refetch()}
                        className="mt-2"
                    >
                        重新打开
                    </Button>
                )}
            </main>
        )

    if (!page)
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center text-sm text-muted-foreground">
                <RefreshCw className="mb-3 h-6 w-6 animate-spin text-primary" />
                正在打开选品页…
            </main>
        )

    if (page.kind === "LOCKED") return <PublicSelectionAccess access={access} />
    if (page.kind === "ENDED") return <PublicSelectionEnded />
    if (page.kind === "RECEIPT" && page.receipt)
        return (
            <PublicSelectionReceipt
                token={token}
                page={page}
                receipt={page.receipt}
                accessToken={access.accessToken}
                onLock={access.lock}
            />
        )

    return (
        <SelectionForm
            key={`${token}:${access.accessToken}`}
            token={token}
            accessToken={access.accessToken}
            onLock={access.lock}
            page={page}
            refresh={async () => (await query.refetch()).data}
        />
    )
}
