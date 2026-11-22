"use client"

import * as React from "react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

import type { InvoiceRequest } from "../api"
import { useInvoiceRequestPermissions } from "../hooks/queries"
import { dismissPendingInvoiceRequestDetail } from "../pending-detail"
import { InvoiceRequestDetail } from "./request-detail"
import { InvoiceRequestForm } from "./request-form"

const LIST_HREF = "/finance/customer-accounts/invoice-requests"

function safeInternalPath(raw: string | null): string | undefined {
    const value = raw?.trim()
    if (!value || !value.startsWith("/") || value.startsWith("//")) {
        return undefined
    }
    return value
}

/** 开票申请独立详情页，承接列表下钻、工作台任务直达与退回后修改重提。 */
export function InvoiceRequestDetailPage({ requestId }: { requestId: string }) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const permissions = useInvoiceRequestPermissions()
    const [editing, setEditing] = React.useState<InvoiceRequest | undefined>()

    const returnTo = safeInternalPath(searchParams.get("returnTo"))
    const hasSourceContext = Boolean(
        returnTo || searchParams.get("from") || searchParams.get("workItemId"),
    )

    const backToList = React.useCallback(() => {
        dismissPendingInvoiceRequestDetail()
        if (returnTo) {
            router.push(returnTo)
            return
        }
        const host = workspaceHost()
        if (host) {
            host.closeCurrent(isWorkspaceFrame() ? window : null, LIST_HREF)
        } else {
            router.push(LIST_HREF)
        }
    }, [returnTo, router])

    if (permissions.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="开票申请详情"
                description="正在核对访问权限…"
                summary
                sidebar
            />
        )
    }

    if (!permissions.canRead && !permissions.canReadDetail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-request-detail-no-read-back",
                        href: returnTo ?? "/workspace",
                        label: returnTo ? "返回来源页" : "返回工作台",
                        visibility: hasSourceContext
                            ? "always"
                            : "standalone-only",
                    }}
                    title="开票申请详情"
                />
                <DetailWorkspaceBody>
                    <div className="mx-auto w-full max-w-3xl py-8">
                        <BusinessFailureState
                            kind="permission"
                            title="当前账号没有查看开票申请的权限。"
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (editing && permissions.canSubmit) {
        return (
            <DetailWorkspacePage>
                <InvoiceRequestForm
                    key={editing.id}
                    existing={editing}
                    presentation="workspace"
                    workspaceChrome={{
                        title: "修改开票申请",
                        statusLabel: "修改中",
                        back: {
                            id: "invoice-request-form-back",
                            label: "返回详情",
                            visibility: "always",
                            onClick: () => setEditing(undefined),
                        },
                    }}
                    onCancel={() => setEditing(undefined)}
                    onDone={() => {
                        dismissPendingInvoiceRequestDetail()
                        setEditing(undefined)
                    }}
                />
            </DetailWorkspacePage>
        )
    }

    return (
        <InvoiceRequestDetail
            key={requestId}
            id={requestId}
            presentation="workspace"
            backLabel={returnTo ? "返回来源页" : "返回申请列表"}
            backVisibility={hasSourceContext ? "always" : "standalone-only"}
            onBack={backToList}
            onEdit={setEditing}
        />
    )
}
