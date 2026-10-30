"use client"

import { useState } from "react"
import { KeyRoundIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { useJdVopAuthorizationQuery } from "../../hooks/jd-vop-authorization"
import {
    JD_VOP_AUTHORIZATION_LABEL,
    jdVopAuthorizationTime,
} from "../../lib/jd-vop-authorization"
import { JdVopTokenDialog } from "../dialogs/jd-vop-token-dialog"
import { JdVopAuthorizationDialog } from "../dialogs/jd-vop-authorization-dialog"

/** 本地授权状态与实际接口连接检查分别展示。 */
export function JdVopAuthorizationSection({
    connectionId,
    disabled,
}: {
    connectionId: string
    disabled: boolean
}) {
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canRead = hasPermission(permissions, "supplier_api_connection:detail")
    const canAuthorize = hasPermission(
        permissions,
        "supplier_api_connection:authorize",
    )
    const query = useJdVopAuthorizationQuery(connectionId, canRead)
    const [open, setOpen] = useState(false)
    const [tokensOpen, setTokensOpen] = useState(false)
    const [submissionUnknown, setSubmissionUnknown] = useState(false)
    const status = query.data
    const prefix = `supplier-api-connection-${toAutomationIdSegment(connectionId)}-jd-vop-authorization`
    const waiting =
        status &&
        ["authorizing", "refreshing", "unknown"].includes(status.state)
    const pollExpired =
        Date.now() >= query.pollUntil && (waiting || query.unconfirmed)
    const refresh = () => {
        query.restartPolling()
        void query.refetch()
    }
    return (
        <section aria-label="京东账号授权" className="space-y-4 p-6">
            <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 space-y-2">
                    <h2 className="flex items-center gap-2 text-base font-semibold">
                        <KeyRoundIcon
                            className="size-4 text-muted-foreground"
                            aria-hidden="true"
                        />
                        京东账号授权
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        授权企业采购账号或配置已有令牌后，可继续测试京东接口连接。
                    </p>
                </div>
                <div className="flex flex-wrap gap-2">
                    <LoadingButton
                        id={`${prefix}-refresh`}
                        type="button"
                        variant="outline"
                        loading={query.isFetching}
                        disabled={!canRead || disabled || query.isFetching}
                        onClick={refresh}
                    >
                        刷新授权状态
                    </LoadingButton>
                    <Button
                        id={`${prefix}-configure-tokens`}
                        type="button"
                        variant="outline"
                        disabled={
                            !canAuthorize ||
                            !status?.can_configure_tokens ||
                            query.isError ||
                            query.isFetching ||
                            query.unconfirmed ||
                            disabled
                        }
                        onClick={() => setTokensOpen(true)}
                    >
                        配置令牌
                    </Button>
                    <Button
                        id={`${prefix}-authorize`}
                        type="button"
                        disabled={
                            !canAuthorize ||
                            !status?.can_authorize ||
                            query.isError ||
                            query.isFetching ||
                            query.unconfirmed ||
                            disabled
                        }
                        onClick={() => setOpen(true)}
                    >
                        {status?.state === "missing"
                            ? "授权京东账号"
                            : "重新授权"}
                    </Button>
                </div>
            </div>
            {!canRead ? (
                <p className="text-sm text-muted-foreground">
                    当前账号没有查看京东授权状态的权限，请联系管理员。
                </p>
            ) : query.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在查询京东授权状态…
                </p>
            ) : query.isError ? (
                <p role="alert" className="text-sm text-destructive">
                    暂时无法查询授权状态，请刷新；如仍失败，请联系管理员核验权限和服务配置。
                </p>
            ) : status ? (
                <div className="space-y-3 text-sm" aria-live="polite">
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
                        <Badge
                            variant={
                                ["authorized", "configured"].includes(
                                    status.state,
                                ) && !query.unconfirmed
                                    ? "success"
                                    : status.state === "missing"
                                      ? "secondary"
                                      : "warning"
                            }
                        >
                            {
                                JD_VOP_AUTHORIZATION_LABEL[
                                    query.unconfirmed ? "unknown" : status.state
                                ]
                            }
                        </Badge>
                        {status.expires_at !== null && !query.unconfirmed ? (
                            <span className="text-muted-foreground">
                                到期时间：
                                {jdVopAuthorizationTime(status.expires_at)}
                            </span>
                        ) : null}
                    </div>
                    <p className="text-muted-foreground">
                        {query.unconfirmed
                            ? "本次授权结果待查询，请先刷新状态确认处理进度。"
                            : status.message}
                    </p>
                    {["authorized", "configured"].includes(status.state) &&
                    !query.unconfirmed ? (
                        <p className="text-muted-foreground">
                            授权已保存，请测试连接。接口连接结果以最近一次测试为准，授权不会开启自动下单。
                        </p>
                    ) : null}
                    {status.recovery_after !== null && !status.can_authorize ? (
                        <p className="text-muted-foreground">
                            请先查询原授权结果，
                            {jdVopAuthorizationTime(status.recovery_after)}{" "}
                            后再刷新状态查看可处理方式。
                        </p>
                    ) : null}
                </div>
            ) : null}
            {submissionUnknown && query.unconfirmed ? (
                <p role="alert" className="text-sm text-destructive">
                    本次授权结果尚未查明，本次敏感输入已清空。请刷新授权状态，不要重复提交；仍无法确认时请联系管理员核实。
                </p>
            ) : null}
            {pollExpired ? (
                <p role="status" className="text-sm text-muted-foreground">
                    已停止自动查询。可稍后点击“刷新授权状态”，或关闭页面后回来查看。
                </p>
            ) : waiting || query.unconfirmed ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在自动查询授权进度，最多查询 2
                    分钟。可离开页面后回来查看。
                </p>
            ) : null}
            {!canAuthorize ? (
                <p className="text-sm text-muted-foreground">
                    当前账号没有办理京东授权的权限，请联系管理员。
                </p>
            ) : null}
            {tokensOpen && status ? (
                <JdVopTokenDialog
                    connectionId={connectionId}
                    status={status}
                    idPrefix={`${prefix}-tokens`}
                    onClose={() => setTokensOpen(false)}
                    onSubmitted={(operationId, uncertain) => {
                        setTokensOpen(false)
                        setSubmissionUnknown(uncertain)
                        query.restartPolling(uncertain ? operationId : null)
                        void query.refetch()
                    }}
                />
            ) : null}
            {open && status ? (
                <JdVopAuthorizationDialog
                    connectionId={connectionId}
                    status={status}
                    idPrefix={prefix}
                    onClose={() => setOpen(false)}
                    onSubmitted={(operationId, uncertain) => {
                        setOpen(false)
                        setSubmissionUnknown(uncertain)
                        query.restartPolling(uncertain ? operationId : null)
                        void query.refetch()
                    }}
                />
            ) : null}
        </section>
    )
}
