"use client"
import { useRef, useState } from "react"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import { portalApplicationAction } from "../api"
import {
    usePortalApplication,
    usePortalCommand,
    usePortalCooperation,
} from "../hooks/queries"
import {
    commandKey,
    isRejectedPortalCommand,
    kindLabels,
    statusLabels,
} from "../lib/presentation"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError, PortalSurface } from "../components/surface"
import { PortalApplicationContent } from "../components/application-content"
import { PortalTermsForm } from "../components/terms-form"
import { PortalStopForm } from "../components/stop-form"
import { PortalCooperationEditor } from "./cooperation-page"
import { PortalNewProductEditor } from "./new-product-page"
import { usePortalProfile } from "../components/portal-session"
export function PortalApplicationDetailPage({
    applicationId,
}: {
    applicationId: string
}) {
    const query = usePortalApplication(applicationId)
    const profile = usePortalProfile()
    const cooperation = usePortalCooperation(query.data?.kind === "cooperation")
    const [editing, setEditing] = useState(false)
    const [error, setError] = useState<unknown>(null)
    const intent = useRef<{
        action: "submit" | "withdraw"
        body: Record<string, unknown>
    } | null>(null)
    const command = usePortalCommand(
        (input: {
            action: "submit" | "withdraw"
            body: Record<string, unknown>
        }) => portalApplicationAction(applicationId, input.action, input.body),
    )
    const execute = async (action: "submit" | "withdraw") => {
        if (!query.data) return
        if (intent.current && intent.current.action !== action) {
            setError(new Error("请先确认上次操作结果，再执行其他操作"))
            return
        }
        intent.current ??= {
            action,
            body: {
                expected_version: query.data.version,
                idempotency_key: commandKey(action),
            },
        }
        setError(null)
        try {
            await command.mutateAsync(intent.current)
            intent.current = null
            setEditing(false)
        } catch (cause) {
            if (isRejectedPortalCommand(cause)) intent.current = null
            setError(cause)
        }
    }
    if (!query.data)
        return (
            <PortalSurface title="申请资料">
                <PortalError
                    error={query.error}
                    retry={() => void query.refetch()}
                />
                {query.isPending && <p>正在读取申请…</p>}
            </PortalSurface>
        )
    const application = query.data
    const editable = ["draft", "returned", "withdrawn"].includes(
        application.status,
    )
    const writable = profile?.role === "maintainer"
    return (
        <PortalSurface
            title={
                application.title ??
                kindLabels[application.kind] ??
                "供应合作申请"
            }
            description={`当前状态：${statusLabels[application.status] ?? "待核对"}`}
            actions={
                <Link
                    id="supplier-portal-application-back"
                    href="/supplier-portal/applications"
                    className="text-sm text-primary"
                >
                    返回我的申请
                </Link>
            }
        >
            <PortalError
                error={error ?? query.error}
                retry={() => void query.refetch()}
                id="supplier-portal-application-reload"
            />
            <PortalCommandConflict
                error={error}
                id="supplier-portal-application-recheck-conflict"
                currentSummary={`当前申请状态：${statusLabels[application.status] ?? "待核对"}。下方为当前申请资料。`}
                disabled={command.isPending}
                onRecheck={async () => {
                    const result = await query.refetch()
                    if (!result.data || result.isError)
                        throw (
                            result.error ??
                            new Error("申请状态暂不可用，请重新读取")
                        )
                }}
                onConfirmed={() => {
                    intent.current = null
                    setError(null)
                }}
            />
            <PortalApplicationContent application={application} />
            <div className="flex flex-wrap gap-2">
                {editable && (
                    <>
                        <Button
                            id="supplier-portal-application-edit"
                            variant="outline"
                            disabled={
                                !writable ||
                                command.isPending ||
                                !!intent.current
                            }
                            onClick={() => setEditing(!editing)}
                        >
                            {editing ? "收起编辑" : "修改申请"}
                        </Button>
                        <Button
                            id="supplier-portal-application-submit"
                            disabled={
                                !writable ||
                                editing ||
                                command.isPending ||
                                (intent.current != null &&
                                    intent.current.action !== "submit")
                            }
                            onClick={() => void execute("submit")}
                        >
                            {intent.current?.action === "submit"
                                ? "重试原提交"
                                : "提交采购确认"}
                        </Button>
                    </>
                )}
                {application.status === "pending" && (
                    <Button
                        id="supplier-portal-application-withdraw"
                        variant="outline"
                        disabled={
                            !writable ||
                            command.isPending ||
                            (intent.current != null &&
                                intent.current.action !== "withdraw")
                        }
                        onClick={() => void execute("withdraw")}
                    >
                        {intent.current?.action === "withdraw"
                            ? "重试原撤回"
                            : "撤回本次申请"}
                    </Button>
                )}
            </div>
            {editing && application.kind === "new_product" && (
                <PortalNewProductEditor
                    key={application.id}
                    draft={application}
                />
            )}
            {editing &&
                (application.kind === "quote" ||
                    application.kind === "terms") && (
                    <PortalTermsForm key={application.id} draft={application} />
                )}
            {editing && application.kind === "stop" && (
                <PortalStopForm key={application.id} draft={application} />
            )}
            {editing && application.kind === "cooperation" && (
                <>
                    <PortalError
                        error={cooperation.error}
                        retry={() => void cooperation.refetch()}
                        id="supplier-portal-cooperation-draft-retry"
                    />
                    {cooperation.data && (
                        <PortalCooperationEditor
                            key={application.id}
                            draft={application}
                            profile={cooperation.data}
                        />
                    )}
                </>
            )}
        </PortalSurface>
    )
}
