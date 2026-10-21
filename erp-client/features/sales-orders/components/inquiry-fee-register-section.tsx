"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    MoneyValue,
    RateValue,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { formatDateTime } from "@/lib/datetime"
import {
    readInquiryFileValue,
    removeInquiryFileValue,
    writeInquiryFileValue,
} from "@/lib/inquiry-file-store"
import { hasPermission } from "@/lib/permissions"
import {
    fetchSalesInquiryFeeCandidates,
    type ExtraCostCategory,
    type RegisterSalesExtraCostInput,
    type SalesInquiryFeeCandidate,
} from "../api/sales-order-extra-costs"
import { useRegisterSalesExtraCostMutation } from "../hooks/queries"
import {
    EXTRA_COST_CATEGORY_LABEL,
    EXTRA_COST_WHOLE_ORDER,
    registerDefaults,
    registerExtraCostSchema,
    todayDate,
    type RegisterExtraCostValues,
} from "../lib/extra-cost"

type StoredRegistration = {
    input: RegisterSalesExtraCostInput
    draft_key: string
}

function categoryCode(value: string): ExtraCostCategory | undefined {
    const code = value === "warehousing" ? "storage" : value
    return Object.hasOwn(EXTRA_COST_CATEGORY_LABEL, code)
        ? (code as ExtraCostCategory)
        : undefined
}

function candidateReason(candidate: SalesInquiryFeeCandidate): string | null {
    return (
        candidate.unavailable_reason ??
        (!categoryCode(candidate.category)
            ? "费用类别暂不支持，请重新确认费用"
            : null)
    )
}

function sourceKey(userId: string, salesOrderId: string): string {
    return `inquiry-finance:${encodeURIComponent(userId)}:${encodeURIComponent(salesOrderId)}`
}

/** 财务通过自身销售范围读取费用候选，明确选择后消费原询价分摊。 */
export function InquiryFeeRegisterSection({
    salesOrderId,
    salesOrderNo,
    onRegistered,
}: {
    salesOrderId: string
    salesOrderNo: string
    onRegistered: () => void
}) {
    const profile = useAccountProfileQuery()
    const allowed = hasPermission(
        profile.data?.permissions,
        "cost_entry:create",
    )
    const query = useQuery({
        queryKey: ["sales-orders", "inquiry-fees", salesOrderId],
        queryFn: () => fetchSalesInquiryFeeCandidates(salesOrderId),
        enabled: allowed,
        staleTime: 0,
    })
    const mutation = useRegisterSalesExtraCostMutation(salesOrderId)
    const [selected, setSelected] =
        React.useState<SalesInquiryFeeCandidate | null>(null)
    const [pending, setPending] = React.useState<StoredRegistration | null>(
        null,
    )
    const [ready, setReady] = React.useState(false)
    const [failure, setFailure] = React.useState<string | null>(null)
    const commandKey = `${sourceKey(profile.data?.userid ?? "", salesOrderId)}:command`
    React.useEffect(() => {
        let active = true
        setReady(false)
        setPending(null)
        if (!profile.data?.userid) return
        void readInquiryFileValue<StoredRegistration>(commandKey)
            .then((stored) => {
                if (!active) return
                setPending(stored ?? null)
                setReady(true)
            })
            .catch(() => {
                if (active)
                    setFailure("无法读取上次费用及凭证，请检查本机存储后重试。")
            })
        return () => {
            active = false
        }
    }, [commandKey, profile.data?.userid])

    async function submit(command: StoredRegistration) {
        if (!allowed || !ready || !profile.data?.userid)
            throw new Error("当前费用登记资格尚未核对，请稍候重试。")
        setFailure(null)
        // File 以结构化数据持久保存，原请求未保存成功时禁止发送。
        await writeInquiryFileValue(commandKey, command)
        setPending(command)
        try {
            await mutation.mutateAsync(command.input)
        } catch (error) {
            if (classifyFormalCommandError(error) === "failed") {
                await removeInquiryFileValue(commandKey)
                setPending(null)
            }
            setFailure(
                getErrorMessage(error, "费用登记失败，请保留原凭证后重试。"),
            )
            throw error
        }
        await Promise.allSettled([
            removeInquiryFileValue(commandKey),
            removeInquiryFileValue(command.draft_key),
        ])
        setPending(null)
        setSelected(null)
        await query.refetch()
        onRegistered()
    }

    if (!allowed) return null
    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-label="财务询价费用来源"
        >
            <div>
                <h3 className="font-medium">登记已接受询价费用</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                    只办理已在 ERP 外支付或无需 ERP
                    付款的费用。已确认询价费用须明确选择原来源，下方手工登记不会按费用名称自动关联。
                </p>
            </div>
            {pending ? (
                <Alert variant="warning">
                    <AlertTitle>上次费用登记结果待确认</AlertTitle>
                    <AlertDescription>
                        已保留原金额、费用来源和凭证，请先核对原结果。
                    </AlertDescription>
                    <Button
                        id="finance-inquiry-fee-recover"
                        type="button"
                        disabled={mutation.isPending || !ready}
                        onClick={() => {
                            void submit(pending).catch(() => undefined)
                        }}
                    >
                        使用原请求及凭证核对
                    </Button>
                </Alert>
            ) : null}
            {failure ? (
                <Alert variant="destructive">
                    <AlertDescription>{failure}</AlertDescription>
                </Alert>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    id="finance-inquiry-fee-retry"
                    title="询价费用来源加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isPending ? (
                <p role="status">正在核对可登记费用…</p>
            ) : query.data.items.length ? (
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>询价费用</TableHead>
                            <TableHead>可登记金额</TableHead>
                            <TableHead>办理条件</TableHead>
                            <TableHead>操作</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {query.data.items.map((candidate) => {
                            const reason = candidateReason(candidate)
                            return (
                                <TableRow
                                    key={`${candidate.inquiry_id}:${candidate.allocation_id}`}
                                >
                                    <TableCell>
                                        <p>{candidate.name}</p>
                                        <p className="text-xs text-muted-foreground">
                                            {candidate.inquiry_no} · 第{" "}
                                            {candidate.feedback_round} 轮
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={candidate.available_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell className="whitespace-normal">
                                        {reason ??
                                            (candidate.valid_conditions ||
                                                "按本轮确认的 ERP 外费用办理")}
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            有效至{" "}
                                            {formatDateTime(
                                                new Date(
                                                    candidate.valid_until *
                                                        1000,
                                                ).toISOString(),
                                            )}
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        <Button
                                            id={`finance-inquiry-fee-select-${toAutomationIdSegment(`${candidate.inquiry_id}-${candidate.allocation_id}`)}`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                Boolean(reason) ||
                                                Boolean(pending) ||
                                                mutation.isPending ||
                                                !ready ||
                                                query.isFetching
                                            }
                                            onClick={() =>
                                                setSelected(candidate)
                                            }
                                        >
                                            选择登记
                                        </Button>
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </Table>
            ) : (
                <p className="text-sm text-muted-foreground">
                    本销售单尚无已明确分摊的询价费用；有询价费用时由销售先确认归属。
                </p>
            )}
            {selected && profile.data ? (
                <InquiryFeeRegistration
                    key={`${selected.inquiry_id}:${selected.allocation_id}`}
                    candidate={selected}
                    salesOrderNo={salesOrderNo}
                    storagePrefix={sourceKey(profile.data.userid, salesOrderId)}
                    pending={mutation.isPending}
                    unresolved={Boolean(pending)}
                    onClose={() => setSelected(null)}
                    onConfirm={submit}
                />
            ) : null}
        </section>
    )
}

function InquiryFeeRegistration({
    candidate,
    salesOrderNo,
    storagePrefix,
    pending,
    unresolved,
    onClose,
    onConfirm,
}: {
    candidate: SalesInquiryFeeCandidate
    salesOrderNo: string
    storagePrefix: string
    pending: boolean
    unresolved: boolean
    onClose: () => void
    onConfirm: (command: StoredRegistration) => Promise<void>
}) {
    const draftKey = `${storagePrefix}:${encodeURIComponent(candidate.allocation_id)}:draft`
    const [initial, setInitial] =
        React.useState<RegisterExtraCostValues | null>(null)
    const [restoreFailed, setRestoreFailed] = React.useState(false)
    React.useEffect(() => {
        let active = true
        void readInquiryFileValue<RegisterExtraCostValues>(draftKey)
            .then((stored) => {
                if (!active) return
                const defaults = {
                    ...registerDefaults(),
                    category: categoryCode(candidate.category) ?? "",
                    description: candidate.name,
                    grossAmount: candidate.available_amount,
                    taxRatePercent: multiplyFixed(candidate.tax_rate, "100", {
                        leftMaxScale: 6,
                        rightMaxScale: 0,
                        outputScale: 2,
                    }),
                }
                setInitial(
                    stored
                        ? {
                              ...stored,
                              category: defaults.category,
                              taxRatePercent: defaults.taxRatePercent,
                          }
                        : defaults,
                )
            })
            .catch(() => {
                if (active) setRestoreFailed(true)
            })
        return () => {
            active = false
        }
        // 同一费用来源的背景刷新不替换已输入资料及凭证。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [draftKey])
    if (restoreFailed)
        return (
            <Alert variant="destructive">
                <AlertDescription>
                    无法恢复该笔费用的本机输入，请检查浏览器存储后重新选择。
                </AlertDescription>
            </Alert>
        )
    return initial ? (
        <InquiryFeeRegistrationForm
            candidate={candidate}
            salesOrderNo={salesOrderNo}
            initial={initial}
            draftKey={draftKey}
            pending={pending}
            unresolved={unresolved}
            onClose={onClose}
            onConfirm={onConfirm}
        />
    ) : null
}

function InquiryFeeRegistrationForm({
    candidate,
    salesOrderNo,
    initial,
    draftKey,
    pending,
    unresolved,
    onClose,
    onConfirm,
}: {
    candidate: SalesInquiryFeeCandidate
    salesOrderNo: string
    initial: RegisterExtraCostValues
    draftKey: string
    pending: boolean
    unresolved: boolean
    onClose: () => void
    onConfirm: (command: StoredRegistration) => Promise<void>
}) {
    const [preview, setPreview] = React.useState<StoredRegistration | null>(
        null,
    )
    const [storageFailure, setStorageFailure] = React.useState(false)
    const saveQueue = React.useRef(Promise.resolve())
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                const snapshot = structuredClone(formApi.state.values)
                saveQueue.current = saveQueue.current
                    .then(() => writeInquiryFileValue(draftKey, snapshot))
                    .then(() => setStorageFailure(false))
                    .catch(() => setStorageFailure(true))
            },
        },
        validators: {
            onChange: registerExtraCostSchema,
            onSubmit: ({ value }) => {
                if (candidateReason(candidate))
                    return {
                        fields: { grossAmount: candidateReason(candidate)! },
                    }
                try {
                    if (
                        compareDecimal(
                            value.grossAmount,
                            candidate.available_amount,
                            2,
                        ) > 0
                    )
                        return {
                            fields: {
                                grossAmount: "登记金额超过本笔费用的剩余额度",
                            },
                        }
                } catch {
                    return { fields: { grossAmount: "请输入有效的金额" } }
                }
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            await saveQueue.current
            if (pending || unresolved || storageFailure || !value.evidence)
                return
            setPreview({
                draft_key: draftKey,
                input: {
                    category: categoryCode(candidate.category)!,
                    description: value.description.trim(),
                    grossAmount: value.grossAmount.trim(),
                    inputTaxRate: candidate.tax_rate,
                    occurredOn: value.occurredOn,
                    supplierId: value.supplierId || undefined,
                    salesOrderLineId:
                        value.salesOrderLineId === EXTRA_COST_WHOLE_ORDER
                            ? undefined
                            : value.salesOrderLineId || undefined,
                    evidence: value.evidence,
                    idempotencyKey: `inquiry-finance:${crypto.randomUUID()}`,
                    inquirySource: {
                        inquiry_id: candidate.inquiry_id,
                        feedback_round: candidate.feedback_round,
                        fee_id: candidate.fee_id,
                        allocation_id: candidate.allocation_id,
                    },
                },
            })
        },
    })
    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open && !pending) onClose()
                }}
            >
                <DialogContent
                    closeButtonId="finance-inquiry-register-close"
                    className="max-h-[90svh] overflow-y-auto sm:max-w-xl"
                >
                    <DialogHeader>
                        <DialogTitle>登记询价来源费用</DialogTitle>
                        <DialogDescription>
                            {candidate.inquiry_no} · {candidate.name}
                            。登记后计入本销售单实际经营盈亏，不生成应付。
                        </DialogDescription>
                    </DialogHeader>
                    <p className="text-sm">
                        类别：
                        {
                            EXTRA_COST_CATEGORY_LABEL[
                                categoryCode(candidate.category)!
                            ]
                        }
                        ；可登记：
                        <MoneyValue
                            value={candidate.available_amount}
                            display="exact"
                        />
                        ；税率：
                        <RateValue
                            value={initial.taxRatePercent}
                            precision={2}
                        />
                    </p>
                    <form
                        className="space-y-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="description">
                            {(field) => (
                                <field.TextField
                                    id="finance-inquiry-register-description"
                                    label="费用说明"
                                    disabled={pending || unresolved}
                                    required
                                />
                            )}
                        </form.AppField>
                        <div className="grid gap-4 sm:grid-cols-2">
                            <form.AppField name="grossAmount">
                                {(field) => (
                                    <field.TextField
                                        id="finance-inquiry-register-amount"
                                        money
                                        label="本次登记含税金额"
                                        inputMode="decimal"
                                        disabled={pending || unresolved}
                                        required
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="occurredOn">
                                {(field) => (
                                    <field.DateField
                                        id="finance-inquiry-register-date"
                                        label="发生日期"
                                        required
                                        clearable={false}
                                        disabled={pending || unresolved}
                                        disabledDates={{
                                            after: new Date(
                                                `${todayDate()}T00:00:00`,
                                            ),
                                        }}
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField name="evidence">
                            {(field) => (
                                <field.EvidenceUploadField
                                    id="finance-inquiry-register-evidence"
                                    label="费用凭证"
                                    required
                                    uploadLabel="上传 ERP 外付款或费用发生凭证"
                                    disabled={pending || unresolved}
                                />
                            )}
                        </form.AppField>
                        <p className="text-sm text-muted-foreground">
                            {candidate.valid_conditions ||
                                "按已接受费用条件登记。"}{" "}
                            原来源会与费用事实一并保存，后续冲减不自动恢复询价费用可用额。
                        </p>
                        {storageFailure ? (
                            <Alert variant="destructive">
                                <AlertDescription>
                                    费用输入或凭证未能保存到本机，请检查存储后重试。当前内容仍保留，尚未登记。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <DialogFooter>
                            <Button
                                id="finance-inquiry-register-cancel"
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={onClose}
                            >
                                返回
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="finance-inquiry-register-preview"
                                    label="预览费用登记"
                                    disabled={
                                        pending || unresolved || storageFailure
                                    }
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <FormalActionConfirmDialog
                idPrefix="finance-inquiry-register-confirm"
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open && !pending) setPreview(null)
                }}
                title="核对询价费用登记"
                actionLabel="登记费用"
                confirmLabel="确认登记费用"
                showStatusChange={false}
                fromStatus={{ label: "ERP 外费用", tone: "info" }}
                toStatus={{ label: "已登记", tone: "success" }}
                pending={pending}
                summary={
                    preview
                        ? [
                              <span key="inquiry">
                                  询价：{candidate.inquiry_no} · 第{" "}
                                  {candidate.feedback_round} 轮
                              </span>,
                              <span key="fee">
                                  费用：{preview.input.description}
                              </span>,
                              <span key="sales">
                                  归属销售单：{salesOrderNo}
                              </span>,
                              <span key="amount">
                                  含税金额：
                                  <MoneyValue
                                      value={preview.input.grossAmount}
                                      display="exact"
                                  />
                              </span>,
                              <span key="date">
                                  发生日期：{preview.input.occurredOn}
                              </span>,
                              <span key="file">
                                  凭证：{preview.input.evidence.name}
                              </span>,
                          ]
                        : []
                }
                effects={[
                    "在同一事务核对来源、分摊余额和费用类别，登记实际费用并占用对应金额。",
                    "本次不生成费用采购或应付；后续财务冲减不自动恢复询价额度。",
                ]}
                onConfirm={async () => {
                    if (!preview) return
                    await onConfirm(preview)
                    setPreview(null)
                }}
            />
        </>
    )
}
