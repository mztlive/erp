"use client"

import { useCallback, useEffect, useRef } from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { InvoiceRegistrationEditor } from "@/features/supplier-payables/components/invoice-registration-editor"
import { nextSessionId } from "@/features/supplier-payables/api/shared"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import { useInvoiceRegistrationDraft } from "@/features/supplier-payables/hooks/use-invoice-registration-draft"
import {
    INVOICE_REGISTRATION_ROUTE,
    invoiceReturnHref,
} from "@/features/supplier-payables/lib/invoice-registration-route"
import { hasPermission } from "@/lib/permissions"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

/** 独立入口只加载当前账号及登记所需事实，不依赖往来列表查询。 */
export function SupplierInvoiceRegistrationPage() {
    const params = useSearchParams()
    const profile = useAccountProfileQuery()
    const router = useRouter()
    const supplierId = params.get("supplierId") ?? ""
    const draftSessionId = params.get("draftSessionId")
    const initialDraftId = useRef<string | null>(null)
    useEffect(() => {
        if (!supplierId || draftSessionId) return
        initialDraftId.current ??= nextSessionId()
        const next = new URLSearchParams(params.toString())
        next.set("draftSessionId", initialDraftId.current)
        router.replace(`${INVOICE_REGISTRATION_ROUTE}?${next}`, {
            scroll: false,
        })
    }, [draftSessionId, params, router, supplierId])
    if (profile.isError && !profile.data)
        return (
            <DetailWorkspacePage>
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessFailureState
                        title="账号信息加载失败"
                        error={profile.error}
                        action={
                            <Button
                                id="invoice-registration-profile-retry"
                                onClick={() => void profile.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                </div>
            </DetailWorkspacePage>
        )
    if (!profile.data)
        return (
            <DetailWorkspacePage>
                <div className="px-(--scaffold-px) py-6">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    if (
        !hasPermission(
            profile.data.permissions,
            "purchase_invoice_allocation:post",
        )
    )
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-registration-no-scope-back",
                        href: "/finance/supplier-accounts",
                        label: "返回供应商往来",
                    }}
                    title="登记进项发票"
                />
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessEmptyState
                        kind="no-scope"
                        title="无进项发票登记权限"
                        description="请联系管理员确认当前账号的财务权限。"
                    />
                </div>
            </DetailWorkspacePage>
        )
    if (!supplierId)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-registration-no-supplier-back",
                        href: "/finance/supplier-accounts",
                        label: "返回供应商往来",
                    }}
                    title="登记进项发票"
                />
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessEmptyState
                        kind="no-data"
                        title="请先选择供应商"
                        description="从供应商往来列表选择供应商后登记发票。"
                        action={
                            <Button
                                id="invoice-registration-pick-supplier"
                                onClick={() =>
                                    router.push("/finance/supplier-accounts")
                                }
                            >
                                返回供应商往来
                            </Button>
                        }
                    />
                </div>
            </DetailWorkspacePage>
        )
    if (!draftSessionId)
        return (
            <DetailWorkspacePage>
                <div className="px-(--scaffold-px) py-6">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    return (
        <InvoiceRegistrationSession
            key={`${profile.data.userid}:${supplierId}:${params.get("invoiceId") ?? "new"}:${draftSessionId}`}
            userId={profile.data.userid}
            supplierId={supplierId}
        />
    )
}

function InvoiceRegistrationSession({
    userId,
    supplierId,
}: {
    userId: string
    supplierId: string
}) {
    const params = useSearchParams()
    const router = useRouter()
    const returnTo = invoiceReturnHref(params.get("returnTo"))
    const syncDraftId = useCallback(
        (draftSessionId: string) => {
            const next = new URLSearchParams(params.toString())
            next.set("draftSessionId", draftSessionId)
            router.replace(`${INVOICE_REGISTRATION_ROUTE}?${next}`, {
                scroll: false,
            })
        },
        [params, router],
    )
    const state = useAllocationSession(
        {
            track: "purchase_invoice",
            supplierId,
            draftSessionId: params.get("draftSessionId") ?? undefined,
            purchaseOrderId: params.get("purchaseOrderId") ?? undefined,
            existingInvoiceId: params.get("invoiceId") ?? undefined,
            preselectPayableAccountId:
                params.get("payableAccountId") ?? undefined,
        },
        { onDraftSessionIdChange: syncDraftId },
    )
    const draft = useInvoiceRegistrationDraft(state, userId)
    const close = (href = returnTo) => {
        const host = workspaceHost()
        if (host) host.closeCurrent(isWorkspaceFrame() ? window : null, href)
        else router.push(href)
    }

    return (
        <DetailWorkspacePage>
            {state.sessionQuery.isPending ? (
                <div className="flex flex-col gap-4 px-(--scaffold-px) py-6">
                    <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </div>
            ) : state.sessionQuery.isError ||
              state.session?.supplierLoadError ? (
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessFailureState
                        title="登记信息加载失败"
                        error={
                            state.sessionQuery.error ??
                            state.session?.supplierLoadError
                        }
                        action={
                            <Button
                                id="invoice-registration-session-retry"
                                variant="outline"
                                onClick={() =>
                                    void state.sessionQuery.refetch()
                                }
                            >
                                重试
                            </Button>
                        }
                    />
                </div>
            ) : (
                <InvoiceRegistrationEditor
                    state={state}
                    draftHint={draft.hint}
                    onSaveDraft={draft.save}
                    onClose={() => close()}
                    onGoToInvoiceView={() => {
                        const next = new URL(returnTo, window.location.origin)
                        next.searchParams.set("view", "purchase_invoice")
                        router.push(`${next.pathname}${next.search}`)
                    }}
                />
            )}
        </DetailWorkspacePage>
    )
}
