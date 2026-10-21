"use client"
import { useEffect, useState } from "react"
import { useRouter } from "next/navigation"
import {
    BusinessFailureState,
    MoneyValue,
    PageHeader,
    PageScaffold,
    QuantityValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { type FormalCommandIdentity } from "@/lib/formal-command"
import { readInquiryLocalValue } from "@/lib/inquiry-local"
import { getErrorMessage } from "@/lib/api/errors"
import type { InquirySalesBasis } from "../api/inquiry-basis"
import { useInquirySalesBasis } from "../hooks/use-inquiry-sales-basis"
import { useCreateSalesOrderMutation } from "../hooks/queries"
import {
    InquirySalesCommandLedger,
    inquirySaleScope,
    inquirySaleDraftKey,
    readInquirySaleCommand,
    type InquirySalePayload,
} from "../lib/inquiry-sales-storage"
import type { CreateSalesOrderFormValues } from "../lib/sales-order-create-model"
import { SalesOrderCreateForm } from "./sales-order-create-form"

export function InquirySalesOrderCreate({
    inquiryId,
    round,
}: {
    inquiryId: string
    round: number
}) {
    const profile = useAccountProfileQuery()
    const userId = profile.data?.userid ?? ""
    const scope = inquirySaleScope(inquiryId, round)
    const basis = useInquirySalesBasis(inquiryId, round)
    const [saved, setSaved] =
        useState<FormalCommandIdentity<InquirySalePayload> | null>(null)
    const [storageError, setStorageError] = useState<unknown>(null)
    const [restoredIdentity, setRestoredIdentity] = useState("")
    useEffect(() => {
        if (!userId) return
        const restore = () => {
            try {
                setSaved(readInquirySaleCommand(userId, scope) ?? null)
                setStorageError(null)
            } catch (error) {
                setStorageError(error)
            }
            setRestoredIdentity(`${userId}:${scope}`)
        }
        restore()
        window.addEventListener("storage", restore)
        return () => window.removeEventListener("storage", restore)
    }, [userId, scope])
    if (profile.isError && !profile.data)
        return (
            <PageScaffold>
                <PageHeader title="当前账号暂时无法核对" />
                <BusinessFailureState
                    error={profile.error}
                    onRetry={() => void profile.refetch()}
                />
            </PageScaffold>
        )
    if (restoredIdentity !== `${userId}:${scope}` || !userId)
        return (
            <PageScaffold>
                <PageHeader
                    title="从询价创建销售单"
                    description="正在核对当前账号和上次开单记录…"
                />
            </PageScaffold>
        )
    if (storageError)
        return (
            <PageScaffold>
                <PageHeader title="上次开单待核对" />
                <BusinessFailureState error={storageError} />
            </PageScaffold>
        )
    if (saved)
        return (
            <InquirySaleRecovery
                command={saved}
                userId={userId}
                scope={scope}
            />
        )
    if (basis.isPending)
        return (
            <PageScaffold>
                <PageHeader
                    title="从询价创建销售单"
                    description="正在核对已接受结果、可转数量和上次操作…"
                />
            </PageScaffold>
        )
    if (!basis.data)
        return (
            <PageScaffold>
                <PageHeader title="询价暂不可用于开单" />
                <BusinessFailureState
                    error={basis.error}
                    onRetry={() => void basis.refetch()}
                    retryLabel="重新核对"
                />
            </PageScaffold>
        )
    return (
        <LoadedInquirySale
            key={`${userId}-${inquiryId}-${round}`}
            basis={basis.data}
            userId={userId}
            scope={scope}
        />
    )
}

function InquirySaleRecovery({
    command,
    userId,
    scope,
}: {
    command: FormalCommandIdentity<InquirySalePayload>
    userId: string
    scope: string
}) {
    const router = useRouter()
    const mutation = useCreateSalesOrderMutation()
    const [recoveryError, setRecoveryError] = useState<unknown>(null)
    const recover = async () => {
        try {
            const result = await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            new InquirySalesCommandLedger(userId, scope).settle(
                "create",
                "succeeded",
            )
            localStorage.removeItem(inquirySaleDraftKey(userId, scope))
            router.replace(`/sales/orders/${result.salesOrderId}`)
        } catch (error) {
            // 业务拒绝不证明刷新前的创建未成功；保留原记录继续核对。
            setRecoveryError(error)
        }
    }
    return (
        <PageScaffold>
            <PageHeader
                title="核对上次开单结果"
                description="上次发送的开单内容已保留。核对会沿用原操作记录；确认前请勿重新创建。"
            />
            <div className="space-y-3 rounded-lg border p-4">
                <p className="text-sm font-medium">
                    销售单号：{command.payload.orderNo}
                </p>
                {command.payload.lineItems.map((line) => (
                    <div
                        key={line.rowKey}
                        className="flex flex-wrap gap-3 text-sm"
                    >
                        <span>{line.name}</span>
                        <QuantityValue value={line.quantity} unit={line.unit} />
                        <span>
                            成交单价{" "}
                            <MoneyValue value={line.unitPriceGross || "0"} />
                        </span>
                        <span>
                            {line.deliveryAllocations?.length ?? 0} 个配送地址
                        </span>
                    </div>
                ))}
                {mutation.error || recoveryError ? (
                    <p role="status" className="text-sm text-destructive">
                        {getErrorMessage(
                            mutation.error ?? recoveryError,
                            "结果仍待核对，请保持当前记录后重试。",
                        )}
                    </p>
                ) : null}
                <Button
                    id="inquiry-sales-create-recover"
                    disabled={mutation.isPending}
                    onClick={() => void recover()}
                >
                    {mutation.isPending ? "正在核对…" : "核对上次开单结果"}
                </Button>
            </div>
        </PageScaffold>
    )
}

/** 首次加载后冻结服务端依据及本地输入；后台刷新不会覆盖用户编辑。 */
function LoadedInquirySale({
    basis,
    userId,
    scope,
}: {
    basis: InquirySalesBasis
    userId: string
    scope: string
}) {
    const [initialBasis] = useState(basis)
    const [values] = useState(() => {
        const value = readInquiryLocalValue<CreateSalesOrderFormValues>(
            inquirySaleDraftKey(userId, scope),
        )
        return value &&
            value.customerId === basis.customer_id &&
            Array.isArray(value.lineItems) &&
            value.lineItems.every(
                (line) =>
                    line &&
                    typeof line.rowKey === "string" &&
                    typeof line.sku === "string" &&
                    typeof line.dueDate === "string",
            )
            ? value
            : undefined
    })
    return (
        <SalesOrderCreateForm
            initialInquiryBasis={initialBasis}
            initialInquiryValues={values}
        />
    )
}
