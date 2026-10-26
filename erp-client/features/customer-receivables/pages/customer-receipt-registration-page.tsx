"use client"

import { useEffect, useRef, useState } from "react"
import { useIsMutating } from "@tanstack/react-query"
import {
    BusinessFailureState,
    FormalActionResult,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { AllocationSessionScreen } from "./components/allocation-session-screen"
import { useAllocationSessionQuery } from "../hooks/queries"
import { useCustomerReceivablesPermissions } from "../hooks/use-customer-receivables-permissions"
import { writeReceiptSession } from "../lib/receipt-session-storage"
import {
    receiptRegistrationHref,
    receiptReturnHref,
} from "../lib/receipt-registration-route"
import type { PostAllocationResult } from "../types"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { getErrorMessage } from "@/lib/api/errors"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

type Success = Extract<PostAllocationResult, { status: "succeeded" }>

/** 独立登记页直接恢复当前账号的会话，不依赖客户往来列表。 */
export function CustomerReceiptRegistrationPage() {
    const params = useSearchParams()
    const sessionId = params.get("sessionId")
    return (
        <ReceiptRegistrationSession
            key={sessionId ?? "missing"}
            sessionId={sessionId}
        />
    )
}

function ReceiptRegistrationSession({
    sessionId,
}: {
    sessionId: string | null
}) {
    const router = useRouter()
    const params = useSearchParams()
    const permissions = useCustomerReceivablesPermissions()
    const pendingOperations = useIsMutating()
    const query = useAllocationSessionQuery(sessionId)
    const [result, setResult] = useState<Success | null>(null)
    const [migrationError, setMigrationError] = useState<string | null>(null)
    const migrated = useRef<string | null>(null)
    const returnTo = receiptReturnHref(
        params.get("returnTo") ?? query.data?.returnContext?.returnTo,
    )
    const close = () => {
        const host = workspaceHost()
        if (host)
            host.closeCurrent(isWorkspaceFrame() ? window : null, returnTo)
        else router.push(returnTo)
    }
    useEffect(() => {
        const current = query.data
        if (
            result ||
            pendingOperations > 0 ||
            current?.mode !== "receipt" ||
            !current.existingFactId ||
            current.recoveryPending ||
            current.status !== "draft" ||
            current.draftSessionId === current.existingFactId ||
            migrated.current === current.draftSessionId
        )
            return
        try {
            writeReceiptSession(
                { ...current, draftSessionId: current.existingFactId },
                null,
            )
            migrated.current = current.draftSessionId
            router.replace(
                receiptRegistrationHref(current.existingFactId, returnTo),
                { scroll: false },
            )
        } catch (error) {
            setMigrationError(
                getErrorMessage(
                    error,
                    "无法更新草稿恢复地址，当前页面仍可继续操作。",
                ),
            )
        }
    }, [query.data, result, pendingOperations, returnTo, router])
    if (result)
        return (
            <PageScaffold density="compact" className="gap-6">
                <PageHeader title="登记回款" />
                <FormalActionResult
                    status="succeeded"
                    title="回款已提交审批"
                    description="审批通过后计入已收并完成核销。"
                    reference={result.factNo}
                    actions={
                        <Button
                            id="receipt-registration-result-return"
                            onClick={close}
                        >
                            {returnTo.startsWith("/sales/orders/")
                                ? "返回销售单"
                                : "返回列表"}
                        </Button>
                    }
                />
            </PageScaffold>
        )
    if (!sessionId || (query.data && query.data.mode !== "receipt"))
        return (
            <PageScaffold density="compact">
                <PageHeader title="登记回款" />
                <BusinessFailureState
                    title="请重新打开回款登记"
                    description="从客户往来或销售单选择结算主体后登记回款。"
                    action={
                        <Button
                            id="receipt-registration-invalid-return"
                            onClick={close}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    return (
        <PageScaffold density="compact" className="gap-6">
            {migrationError ? (
                <p role="alert" className="text-sm text-destructive">
                    {migrationError}
                </p>
            ) : null}
            <AllocationSessionScreen
                embedded
                isPending={query.isPending}
                error={query.error}
                onRetry={() => void query.refetch()}
                session={query.data}
                onBackToList={close}
                onClose={close}
                onPosted={setResult}
                canOperate={permissions.canRegisterReceipt}
                permissionReason={permissions.reason}
            />
        </PageScaffold>
    )
}
