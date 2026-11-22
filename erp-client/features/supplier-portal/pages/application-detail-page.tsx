"use client"
import { useEffect, useRef, useState } from "react"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
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
    portalApplicationStatus,
    statusLabels,
} from "../lib/presentation"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError } from "../components/surface"
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
    const editorRef = useRef<HTMLDivElement>(null)
    useEffect(() => {
        if (editing) editorRef.current?.scrollIntoView({ block: "start" })
    }, [editing])
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
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="申请资料" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <PortalError
                            error={query.error}
                            retry={() => void query.refetch()}
                        />
                        {query.isPending && <p role="status">正在读取申请…</p>}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    const application = query.data
    const editable = ["draft", "returned", "withdrawn"].includes(
        application.status,
    )
    const writable = profile?.role === "maintainer"
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={
                    application.title ??
                    kindLabels[application.kind] ??
                    "供应合作申请"
                }
                primaryStatus={portalApplicationStatus(application.status)}
                back={{
                    id: "supplier-portal-application-back",
                    href: "/supplier-portal/applications",
                    label: "返回我的申请",
                }}
                secondaryActions={
                    <>
                        {writable && editable && (
                            <Button
                                id="supplier-portal-application-edit"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={command.isPending || !!intent.current}
                                onClick={() => setEditing(!editing)}
                            >
                                {editing ? "收起编辑" : "修改申请"}
                            </Button>
                        )}
                        {writable && application.status === "pending" && (
                            <Button
                                id="supplier-portal-application-withdraw"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={
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
                    </>
                }
                primaryAction={
                    writable && editable ? (
                        <Button
                            size="sm"
                            id="supplier-portal-application-submit"
                            disabled={
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
                    ) : undefined
                }
            />
            <DetailWorkspaceBody
                alerts={
                    <>
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
                                        new Error(
                                            "申请状态暂不可用，请重新读取",
                                        )
                                    )
                            }}
                            onConfirmed={() => {
                                intent.current = null
                                setError(null)
                            }}
                        />
                    </>
                }
            >
                <PortalApplicationContent application={application} />
                {writable && editing ? (
                    <DetailWorkspaceSection width="form">
                        <div ref={editorRef} className="scroll-mt-32">
                            {application.kind === "new_product" && (
                                <PortalNewProductEditor
                                    inDetailWorkspace
                                    key={application.id}
                                    draft={application}
                                />
                            )}
                            {(application.kind === "quote" ||
                                application.kind === "terms") && (
                                <PortalTermsForm
                                    key={application.id}
                                    draft={application}
                                />
                            )}
                            {application.kind === "stop" && (
                                <PortalStopForm
                                    key={application.id}
                                    draft={application}
                                />
                            )}
                            {application.kind === "cooperation" && (
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
                        </div>
                    </DetailWorkspaceSection>
                ) : null}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
