"use client"

import { useRef, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { format } from "date-fns"
import { useAppForm, applyApiFormError } from "@/components/form"
import { FormalActionConfirmDialog, MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import {
    SupplierSearchCombobox,
    SettlementPartySearchCombobox,
} from "@/features/entity-selectors"
import {
    compareDecimal,
    parseDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { describeMoney } from "@/lib/money-display"
import { isUncertainDepositCommandError } from "@/features/supplier-deposits/lib/command-error"
import { useDepositCommand } from "../hooks/queries"
import type { AccountDetail, DepositCommand, RechargeRequest } from "../types"

export type DepositAction =
    | "create"
    | "recharge_request"
    | "approve"
    | "receive"
    | "authorize"
    | "reconcile"
    | "revoke"

type FieldSpec = {
    name: string
    label: string
    kind?: "amount" | "time" | "supplier" | "party" | "multiline"
    optional?: boolean
    description?: string
    fullWidth?: boolean
}
type ActionSpec = {
    title: string
    description: string
    submit: string
    successToast: string
    requiresDoubleCheck?: boolean
    fields: FieldSpec[]
}

const specs: Record<DepositAction, ActionSpec> = {
    create: {
        title: "新增预存款账户",
        description:
            "账户按付款主体、供应商、资金账号与人民币独立管理。保存时不登记任何余额。",
        submit: "新增账户",
        successToast: "预存款账户已创建",
        fields: [
            { name: "payer_party_id", label: "我方付款主体", kind: "party" },
            { name: "supplier_id", label: "供应商", kind: "supplier" },
            {
                name: "external_account_ref",
                label: "资金账号",
                description:
                    "填写管理员已核实的预存款资金账号配置编号，不填写登录密码或密钥。",
            },
        ],
    },
    recharge_request: {
        title: "申请充值",
        description:
            "申请批准后登记真实银行付款，再核对供应商到账。申请金额不会直接增加可用余额。",
        submit: "提交充值申请",
        successToast: "充值申请已提交",
        fields: [
            { name: "amount", label: "申请金额（元）", kind: "amount" },
            { name: "reason", label: "充值用途", kind: "multiline" },
        ],
    },
    approve: {
        title: "批准充值申请",
        description:
            "核对本申请的付款主体、供应商、金额及批准依据。充值批准不代替逐单采购付款审批。",
        submit: "批准充值",
        successToast: "充值申请已批准，请继续登记银行付款",
        fields: [
            {
                name: "approval_reference",
                label: "批准依据",
                description: "填写可追溯的审批文件或批准记录编号。",
            },
        ],
    },
    receive: {
        title: "核对充值到账",
        description:
            "核对供应商真实到账交易与原银行付款。到账核实成功后才增加账户可用余额。",
        submit: "下一步：核对到账信息",
        successToast: "充值到账已核实，账户可用余额已更新",
        requiresDoubleCheck: true,
        fields: [
            { name: "external_transaction_id", label: "供应商到账交易号" },
            { name: "evidence_reference", label: "到账核对依据" },
            { name: "received_at", label: "供应商实际到账时间", kind: "time" },
        ],
    },
    authorize: {
        title: "授权自动余额执行",
        description:
            "仅覆盖本账户下已逐单批准的采购付款。授权限额、有效期及资金占用仍须逐项满足。",
        submit: "下一步：核对授权额度",
        successToast: "自动余额执行授权已生效",
        requiresDoubleCheck: true,
        fields: [
            {
                name: "approval_reference",
                label: "财务授权依据",
                fullWidth: true,
            },
            { name: "valid_from", label: "生效时间", kind: "time" },
            { name: "valid_until", label: "到期时间", kind: "time" },
            {
                name: "per_execution_limit",
                label: "单次执行上限（元）",
                kind: "amount",
            },
            {
                name: "cumulative_limit",
                label: "累计执行上限（元）",
                kind: "amount",
            },
        ],
    },
    revoke: {
        title: "撤销自动余额执行授权",
        description:
            "撤销后停止新增占用和未提交的采购下单扣款。原订单调查、已发生扣款及返款继续处理。",
        submit: "撤销授权",
        successToast: "余额执行授权已撤销",
        fields: [],
    },
    reconcile: {
        title: "登记资金对账",
        description:
            "使用同一截止时点的本地及供应商余额核对。对账记录保留差异，不直接覆盖本地余额。",
        submit: "保存对账记录",
        successToast: "资金对账记录已保存",
        fields: [
            { name: "covered_from", label: "对账起始时间", kind: "time" },
            { name: "covered_until", label: "对账截止时间", kind: "time" },
            {
                name: "observed_at",
                label: "供应商余额查询时间",
                kind: "time",
                fullWidth: true,
            },
            {
                name: "remote_balance",
                label: "供应商余额（元）",
                kind: "amount",
            },
            {
                name: "local_balance_at_cutoff",
                label: "同一截止时点本地账面余额（元）",
                kind: "amount",
            },
            {
                name: "evidence_reference",
                label: "对账依据",
                fullWidth: true,
            },
            {
                name: "unresolved_transaction_ids",
                label: "未核实交易号",
                kind: "multiline",
                optional: true,
                fullWidth: true,
                description: "每行一个供应商资金交易号。",
            },
        ],
    },
}

function seconds(value: string) {
    return Math.floor(Date.parse(value) / 1000)
}

function tryParseAmount(value: string | undefined): string | null {
    const trimmed = value?.trim() ?? ""
    if (!trimmed) return null
    try {
        parseDecimal(trimmed, { maxScale: 2, allowNegative: true })
        return trimmed
    } catch {
        return null
    }
}

function schema(spec: ActionSpec) {
    return z.record(z.string(), z.string()).superRefine((values, context) => {
        for (const field of spec.fields) {
            const value = values[field.name]?.trim() ?? ""
            if (!value && field.optional) continue
            let message = value ? "" : `请填写${field.label}`
            if (value && field.kind === "amount") {
                try {
                    const amount = parseDecimal(value, {
                        maxScale: 2,
                        allowNegative: true,
                    }).unscaled
                    const allowZero = [
                        "remote_balance",
                        "local_balance_at_cutoff",
                    ].includes(field.name)
                    if (!allowZero && amount <= BigInt(0))
                        message = "金额必须大于零"
                } catch {
                    message = "请输入最多两位小数的金额"
                }
            }
            if (
                value &&
                field.kind === "time" &&
                !Number.isFinite(seconds(value))
            )
                message = "请选择有效时间"
            if (message)
                context.addIssue({
                    code: "custom",
                    path: [field.name],
                    message,
                })
        }
        if (
            values.valid_from &&
            values.valid_until &&
            seconds(values.valid_from) >= seconds(values.valid_until)
        )
            context.addIssue({
                code: "custom",
                path: ["valid_until"],
                message: "到期时间须晚于生效时间",
            })
        if (
            values.covered_from &&
            values.covered_until &&
            seconds(values.covered_from) >= seconds(values.covered_until)
        )
            context.addIssue({
                code: "custom",
                path: ["covered_until"],
                message: "截止时间须晚于起始时间",
            })
        const perExec = tryParseAmount(values.per_execution_limit)
        const cumulative = tryParseAmount(values.cumulative_limit)
        if (
            perExec &&
            cumulative &&
            compareDecimal(perExec, cumulative, 2) > 0
        ) {
            context.addIssue({
                code: "custom",
                path: ["per_execution_limit"],
                message: "单次执行上限不能大于累计执行上限",
            })
        }
    })
}

function commandFor(
    action: DepositAction,
    values: Record<string, string>,
    operation: string,
    account?: AccountDetail,
    request?: RechargeRequest,
    authorization?: {
        id: string
        version: number
        mode?:
            | { type: "automatic" }
            | { type: "manual"; work_item_id: string; task_version: string }
    },
): DepositCommand {
    const body: Record<string, unknown> = {
        ...values,
        operation_key: operation,
    }
    for (const field of specs[action].fields)
        if (field.kind === "time")
            body[field.name] = seconds(values[field.name])
    if (action === "create")
        return {
            path: "",
            body: {
                operation_key: operation,
                identity: { ...values, currency: "CNY" },
            },
        }
    if (action === "authorize") body.purchase_order_ids = []
    if (action === "reconcile")
        body.unresolved_transaction_ids = values.unresolved_transaction_ids
            .split(/\r?\n/)
            .map((value) => value.trim())
            .filter(Boolean)
    if (action === "receive") body.recharge_request_id = request?.id
    if (action === "approve" || action === "revoke") {
        delete body.operation_key
        body.expected_version =
            action === "approve" ? request?.version : authorization?.version
    }
    const path =
        action === "recharge_request"
            ? "/recharge-requests"
            : action === "approve"
              ? `/recharge-requests/${encodeURIComponent(request?.id ?? "")}/approve`
              : action === "receive"
                ? "/recharges"
                : action === "authorize"
                  ? "/authorizations"
                  : action === "revoke"
                    ? `/authorizations/${encodeURIComponent(authorization?.id ?? "")}/revoke`
                    : "/reconciliations"
    return { accountId: account?.account.id, path, body }
}

export function DepositCommandDialog({
    action,
    account,
    request,
    authorization,
    open,
    onOpenChange,
}: {
    action: DepositAction
    account?: AccountDetail
    request?: RechargeRequest
    authorization?: {
        id: string
        version: number
        mode?:
            | { type: "automatic" }
            | { type: "manual"; work_item_id: string; task_version: string }
    }
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const spec = specs[action]
    const [operation] = useState(() => crypto.randomUUID())
    const frozen = useRef<DepositCommand | null>(null)
    const [unknown, setUnknown] = useState(false)
    const [confirmValues, setConfirmValues] = useState<Record<
        string,
        string
    > | null>(null)
    const mutation = useDepositCommand()

    const defaults: Record<string, string> = Object.fromEntries(
        spec.fields.map((field) => [
            field.name,
            field.kind === "time" &&
            !["valid_until", "covered_from"].includes(field.name)
                ? format(new Date(), "yyyy-MM-dd'T'HH:mm:ss")
                : "",
        ]),
    )

    const executeSubmission = async (values: Record<string, string>) => {
        try {
            const command =
                frozen.current ??
                commandFor(
                    action,
                    values,
                    operation,
                    account,
                    request,
                    authorization,
                )
            frozen.current = command
            await mutation.mutateAsync(command)
            toast.add({
                type: "success",
                title: spec.successToast,
            })
            onOpenChange(false)
        } catch (error) {
            const uncertain = isUncertainDepositCommandError(error)
            setUnknown(uncertain)
            if (!uncertain) {
                frozen.current = null
                setConfirmValues(null)
            }
            applyApiFormError(form, error)
        }
    }

    const form = useAppForm({
        defaultValues: defaults,
        validators: { onSubmit: schema(spec) },
        onSubmit: async ({ value }) => {
            if (spec.requiresDoubleCheck && !unknown && !confirmValues) {
                setConfirmValues({ ...value })
                return
            }
            await executeSubmission(value)
        },
    })

    const remoteBalance = useStore(form.store, (s) => s.values.remote_balance)
    const localBalance = useStore(
        form.store,
        (s) => s.values.local_balance_at_cutoff,
    )

    if (action === "revoke")
        return (
            <FormalActionConfirmDialog
                idPrefix="supplier-deposits-revoke-confirm"
                open={open}
                onOpenChange={(value) => {
                    if (!mutation.isPending) onOpenChange(value)
                }}
                pending={mutation.isPending}
                title={
                    authorization?.mode?.type === "manual"
                        ? "撤销本次人工余额执行授权"
                        : "撤销自动余额执行授权"
                }
                actionLabel="撤销授权"
                confirmLabel={unknown ? "查询原撤销结果" : "确认撤销授权"}
                actionVariant="destructive"
                fromStatus={{
                    label:
                        authorization?.mode?.type === "manual"
                            ? "本次人工执行授权"
                            : "自动执行授权",
                    tone: "info",
                }}
                toStatus={{ label: "已撤销", tone: "neutral" }}
                summary={[
                    <span key="supplier">
                        供应商：{account?.supplier_name}
                    </span>,
                    <span key="payer">
                        付款主体：{account?.payer_party_name}
                    </span>,
                ]}
                effects={[
                    "停止本授权范围内的新占用与尚未提交的采购下单扣款。",
                    "原订单调查、既有占用和已发生的扣款与退款记录继续处理，不自动释放可能已扣款的资金。",
                ]}
                onConfirm={async () => {
                    const command =
                        frozen.current ??
                        commandFor(
                            action,
                            {},
                            operation,
                            account,
                            request,
                            authorization,
                        )
                    frozen.current = command
                    try {
                        await mutation.mutateAsync(command)
                        toast.add({
                            type: "success",
                            title: spec.successToast,
                        })
                        onOpenChange(false)
                    } catch (error) {
                        const uncertain = isUncertainDepositCommandError(error)
                        setUnknown(uncertain)
                        if (!uncertain) frozen.current = null
                        throw error
                    }
                }}
            />
        )

    const twoColumn = action === "authorize" || action === "reconcile"
    const requestReadout = request
        ? describeMoney(request.amount)?.readout
        : null

    const parsedRemote =
        action === "reconcile" ? tryParseAmount(remoteBalance) : null
    const parsedLocal =
        action === "reconcile" ? tryParseAmount(localBalance) : null
    const liveReconcileDiff =
        parsedRemote && parsedLocal
            ? subtractFixed(parsedRemote, parsedLocal, {
                  maxScale: 2,
                  outputScale: 2,
              })
            : null
    const liveReconcileDiffSign = liveReconcileDiff
        ? compareDecimal(liveReconcileDiff, "0", 2)
        : 0

    const contextBanners = (
        <>
            {account && (
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">
                        {account.supplier_name}
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{account.payer_party_name}</span>
                    {request?.request_no ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span className="num">
                                充值单号 {request.request_no}
                            </span>
                        </>
                    ) : null}
                    {request ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span className="inline-flex items-baseline gap-1">
                                <span>申请金额</span>
                                <MoneyValue
                                    value={request.amount}
                                    display="exact"
                                />
                                {requestReadout ? (
                                    <span>（{requestReadout}）</span>
                                ) : null}
                            </span>
                        </>
                    ) : null}
                    {action === "receive" && request?.bank_reference ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span className="num">
                                原银行流水 {request.bank_reference}
                            </span>
                        </>
                    ) : null}
                </div>
            )}
            {action === "reconcile" && account && !confirmValues ? (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/70 bg-muted/35 px-3 py-2 text-xs">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                        <span className="text-muted-foreground">
                            当前系统账面余额参考：
                        </span>
                        <MoneyValue
                            value={account.account.book_balance}
                            display="exact"
                        />
                        {describeMoney(account.account.book_balance)
                            ?.readout ? (
                            <span className="text-muted-foreground">
                                （
                                {
                                    describeMoney(account.account.book_balance)
                                        ?.readout
                                }
                                ）
                            </span>
                        ) : null}
                    </div>
                    <Button
                        id="supplier-deposits-reconcile-fill-book-balance"
                        type="button"
                        variant="outline"
                        size="xs"
                        disabled={unknown || mutation.isPending}
                        onClick={() =>
                            form.setFieldValue(
                                "local_balance_at_cutoff",
                                account.account.book_balance,
                            )
                        }
                    >
                        填入当前账面余额
                    </Button>
                </div>
            ) : null}
            {unknown && (
                <Alert variant="warning">
                    <AlertDescription>
                        本次保存结果尚未查明。请刷新账户核对原结果；再次提交将查询或恢复原申请，当前资料已锁定。
                    </AlertDescription>
                </Alert>
            )}
        </>
    )

    return (
        <Dialog
            open={open}
            onOpenChange={(value) => {
                if (!mutation.isPending) onOpenChange(value)
            }}
        >
            <DialogContent
                size={twoColumn ? "lg" : "md"}
                scroll="body"
                closeButtonId={`supplier-deposits-${action}-close`}
            >
                <DialogHeader>
                    <DialogTitle>
                        {confirmValues
                            ? `${spec.title} · 二次核对`
                            : spec.title}
                    </DialogTitle>
                    <DialogDescription>
                        {confirmValues
                            ? "本次属于高风险资金变动操作，提交前请再次核对关键金额与依据。"
                            : spec.description}
                    </DialogDescription>
                </DialogHeader>

                {confirmValues ? (
                    <>
                        <DialogBody className="space-y-4">
                            {contextBanners}
                            <Alert variant="warning">
                                <AlertTitle>请确认以下资金变更信息</AlertTitle>
                                <AlertDescription>
                                    {action === "receive"
                                        ? "确认后将立即增加预存款账户可用余额并写入不可变资金流水。"
                                        : "确认后将生效自动余额执行授权，系统将在有效期内按限额执行已批准的采购付款。"}
                                </AlertDescription>
                            </Alert>
                            <dl className="grid gap-3 rounded-lg border border-border/70 bg-muted/30 p-4 text-body-compact sm:grid-cols-2">
                                {action === "receive" && request ? (
                                    <>
                                        <div>
                                            <dt className="text-xs text-muted-foreground">
                                                到账增加金额
                                            </dt>
                                            <dd className="mt-1 space-y-0.5">
                                                <MoneyValue
                                                    value={request.amount}
                                                    display="exact"
                                                />
                                                {requestReadout ? (
                                                    <div className="text-xs text-muted-foreground">
                                                        {requestReadout}
                                                    </div>
                                                ) : null}
                                            </dd>
                                        </div>
                                        <div>
                                            <dt className="text-xs text-muted-foreground">
                                                原银行流水号
                                            </dt>
                                            <dd className="num mt-1">
                                                {request.bank_reference || "—"}
                                            </dd>
                                        </div>
                                    </>
                                ) : null}
                                {spec.fields.map((specField) => {
                                    const rawVal = confirmValues[specField.name]
                                    const readout =
                                        specField.kind === "amount"
                                            ? describeMoney(rawVal)?.readout
                                            : null
                                    return (
                                        <div
                                            key={specField.name}
                                            className={
                                                specField.fullWidth
                                                    ? "sm:col-span-2"
                                                    : undefined
                                            }
                                        >
                                            <dt className="text-xs text-muted-foreground">
                                                {specField.label}
                                            </dt>
                                            <dd className="mt-1">
                                                {specField.kind === "amount" ? (
                                                    <div className="space-y-0.5">
                                                        <MoneyValue
                                                            value={rawVal}
                                                            display="exact"
                                                        />
                                                        {readout ? (
                                                            <div className="text-xs text-muted-foreground">
                                                                {readout}
                                                            </div>
                                                        ) : null}
                                                    </div>
                                                ) : (
                                                    <span
                                                        className={
                                                            specField.kind ===
                                                            "time"
                                                                ? "num"
                                                                : undefined
                                                        }
                                                    >
                                                        {rawVal || "—"}
                                                    </span>
                                                )}
                                            </dd>
                                        </div>
                                    )
                                })}
                            </dl>
                        </DialogBody>
                        <DialogFooter>
                            <Button
                                id={`supplier-deposits-${action}-back`}
                                type="button"
                                variant="outline"
                                disabled={mutation.isPending || unknown}
                                onClick={() => setConfirmValues(null)}
                            >
                                返回修改
                            </Button>
                            <LoadingButton
                                id={`supplier-deposits-${action}-confirm`}
                                type="button"
                                loading={mutation.isPending}
                                disabled={mutation.isPending}
                                onClick={() =>
                                    void executeSubmission(confirmValues)
                                }
                            >
                                {unknown
                                    ? "查询原保存结果"
                                    : action === "receive"
                                      ? "确认已核实到账"
                                      : "确认保存财务授权"}
                            </LoadingButton>
                        </DialogFooter>
                    </>
                ) : (
                    <form
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <DialogBody className="flex flex-col gap-4">
                            {contextBanners}
                            <div
                                className={
                                    twoColumn
                                        ? "grid gap-4 sm:grid-cols-2"
                                        : "flex flex-col gap-4"
                                }
                            >
                                {spec.fields.map((specField) => (
                                    <div
                                        key={specField.name}
                                        className={
                                            twoColumn && specField.fullWidth
                                                ? "sm:col-span-2"
                                                : undefined
                                        }
                                    >
                                        <form.AppField name={specField.name}>
                                            {(field) => {
                                                const id = `supplier-deposits-${action}-${specField.name.replaceAll("_", "-")}`
                                                if (
                                                    specField.kind ===
                                                        "party" ||
                                                    specField.kind ===
                                                        "supplier"
                                                )
                                                    return (
                                                        <Field>
                                                            <FieldLabel
                                                                htmlFor={id}
                                                            >
                                                                {
                                                                    specField.label
                                                                }
                                                            </FieldLabel>
                                                            {specField.kind ===
                                                            "party" ? (
                                                                <SettlementPartySearchCombobox
                                                                    id={id}
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value ||
                                                                        undefined
                                                                    }
                                                                    onValueChange={(
                                                                        value,
                                                                    ) =>
                                                                        field.handleChange(
                                                                            value ??
                                                                                "",
                                                                        )
                                                                    }
                                                                    disabled={
                                                                        unknown ||
                                                                        mutation.isPending
                                                                    }
                                                                />
                                                            ) : (
                                                                <SupplierSearchCombobox
                                                                    id={id}
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value ||
                                                                        undefined
                                                                    }
                                                                    onValueChange={(
                                                                        value,
                                                                    ) =>
                                                                        field.handleChange(
                                                                            value ??
                                                                                "",
                                                                        )
                                                                    }
                                                                    disabled={
                                                                        unknown ||
                                                                        mutation.isPending
                                                                    }
                                                                />
                                                            )}
                                                            {field.state.meta
                                                                .errors
                                                                .length ? (
                                                                <FieldError
                                                                    errors={field.state.meta.errors.map(
                                                                        (
                                                                            error,
                                                                        ) => ({
                                                                            message:
                                                                                typeof error ===
                                                                                "string"
                                                                                    ? error
                                                                                    : error?.message,
                                                                        }),
                                                                    )}
                                                                />
                                                            ) : null}
                                                        </Field>
                                                    )
                                                if (
                                                    specField.kind ===
                                                    "multiline"
                                                )
                                                    return (
                                                        <field.TextareaField
                                                            id={id}
                                                            label={
                                                                specField.label
                                                            }
                                                            description={
                                                                specField.description
                                                            }
                                                            disabled={
                                                                unknown ||
                                                                mutation.isPending
                                                            }
                                                            required={
                                                                !specField.optional
                                                            }
                                                        />
                                                    )
                                                if (specField.kind === "time")
                                                    return (
                                                        <field.DateTimeField
                                                            id={id}
                                                            label={
                                                                specField.label
                                                            }
                                                            description={
                                                                specField.description
                                                            }
                                                            disabled={
                                                                unknown ||
                                                                mutation.isPending
                                                            }
                                                            required={
                                                                !specField.optional
                                                            }
                                                        />
                                                    )
                                                return (
                                                    <field.TextField
                                                        id={id}
                                                        label={specField.label}
                                                        type="text"
                                                        money={
                                                            specField.kind ===
                                                            "amount"
                                                        }
                                                        inputClassName={
                                                            specField.kind ===
                                                            "amount"
                                                                ? "num text-right"
                                                                : undefined
                                                        }
                                                        inputMode={
                                                            specField.kind ===
                                                            "amount"
                                                                ? "decimal"
                                                                : undefined
                                                        }
                                                        description={
                                                            specField.description
                                                        }
                                                        disabled={
                                                            unknown ||
                                                            mutation.isPending
                                                        }
                                                        required={
                                                            !specField.optional
                                                        }
                                                    />
                                                )
                                            }}
                                        </form.AppField>
                                    </div>
                                ))}
                            </div>
                            {action === "reconcile" &&
                            liveReconcileDiff !== null ? (
                                <Alert
                                    variant={
                                        liveReconcileDiffSign === 0
                                            ? "info"
                                            : "warning"
                                    }
                                >
                                    <AlertTitle>
                                        {liveReconcileDiffSign === 0
                                            ? "实时差额试算：双方余额一致"
                                            : "实时差额试算：存在对账差异"}
                                    </AlertTitle>
                                    <AlertDescription className="flex flex-wrap items-baseline gap-2">
                                        <span>供应商余额 − 本地账面余额 =</span>
                                        <span
                                            className={
                                                liveReconcileDiffSign !== 0
                                                    ? "text-destructive"
                                                    : undefined
                                            }
                                        >
                                            <MoneyValue
                                                value={liveReconcileDiff}
                                                display="exact"
                                                inheritColor={
                                                    liveReconcileDiffSign !== 0
                                                }
                                            />
                                        </span>
                                        {describeMoney(liveReconcileDiff)
                                            ?.readout ? (
                                            <span className="text-xs text-muted-foreground">
                                                （
                                                {
                                                    describeMoney(
                                                        liveReconcileDiff,
                                                    )?.readout
                                                }
                                                ）
                                            </span>
                                        ) : null}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                        </DialogBody>
                        <DialogFooter>
                            <Button
                                id={`supplier-deposits-${action}-cancel`}
                                type="button"
                                variant="outline"
                                disabled={mutation.isPending}
                                onClick={() => onOpenChange(false)}
                            >
                                关闭
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`supplier-deposits-${action}-submit`}
                                    label={
                                        unknown ? "查询原保存结果" : spec.submit
                                    }
                                    loading={mutation.isPending}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                )}
            </DialogContent>
        </Dialog>
    )
}
