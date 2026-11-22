"use client"

import * as React from "react"

import {
    BusinessFailureState,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
} from "@/components/business"
import { useRouter } from "@/lib/navigation"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

import type { InvoiceRequest } from "../api"
import { useInvoiceRequestPermissions } from "../hooks/queries"
import { dismissPendingInvoiceRequestDetail } from "../pending-detail"
import { InvoiceRequestDetail } from "./request-detail"
import { InvoiceRequestForm } from "./request-form"

const LIST_HREF = "/finance/customer-accounts/invoice-requests"

/** 开票申请独立详情页，承接列表下钻、工作台任务直达与退回后修改重提。 */
export function InvoiceRequestDetailPage({ requestId }: { requestId: string }) {
    const router = useRouter()
    const permissions = useInvoiceRequestPermissions()
    const [editing, setEditing] = React.useState<InvoiceRequest | undefined>()

    const backToList = React.useCallback(() => {
        dismissPendingInvoiceRequestDetail()
        const host = workspaceHost()
        if (host) {
            host.closeCurrent(isWorkspaceFrame() ? window : null, LIST_HREF)
        } else {
            router.push(LIST_HREF)
        }
    }, [router])

    if (permissions.isPending) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-request-detail-loading-back",
                        href: LIST_HREF,
                        label: "返回开票申请",
                    }}
                    title="开票申请详情"
                />
                <div className="space-y-4 px-(--scaffold-px) py-6">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-64 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (!permissions.canRead && !permissions.canReadDetail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-request-detail-no-read-back",
                        href: "/workspace",
                        label: "返回工作台",
                    }}
                    title="开票申请详情"
                />
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessFailureState
                        kind="permission"
                        title="当前账号没有查看开票申请的权限。"
                    />
                </div>
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
            onBack={backToList}
            onEdit={setEditing}
        />
    )
}
