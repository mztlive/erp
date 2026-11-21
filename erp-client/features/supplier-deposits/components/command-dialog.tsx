"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { format } from "date-fns"
import { useAppForm, applyApiFormError } from "@/components/form"
import { FormalActionConfirmDialog } from "@/components/business"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    SupplierSearchCombobox,
    SettlementPartySearchCombobox,
} from "@/features/entity-selectors"
import { parseDecimal } from "@/lib/fixed-decimal"
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
}
type ActionSpec = {
    title: string
    description: string
    submit: string
    fields: FieldSpec[]
}
const specs: Record<DepositAction, ActionSpec> = {
    create: {
        title: "新增预存款账户",
        description:
            "账户按付款主体、供应商、资金账号与人民币独立管理。保存时不登记任何余额。",
        submit: "新增账户",
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
        submit: "确认已核实到账",
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
        submit: "保存财务授权",
        fields: [
            { name: "approval_reference", label: "财务授权依据" },
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
        fields: [],
    },
    reconcile: {
        title: "登记资金对账",
        description:
            "使用同一截止时点的本地及供应商余额核对。对账记录保留差异，不直接覆盖本地余额。",
        submit: "保存对账记录",
        fields: [
            { name: "covered_from", label: "对账起始时间", kind: "time" },
            { name: "covered_until", label: "对账截止时间", kind: "time" },
            { name: "observed_at", label: "供应商余额查询时间", kind: "time" },
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
                name: "unresolved_transaction_ids",
                label: "未核实交易号",
                kind: "multiline",
                optional: true,
                description: "每行一个供应商资金交易号。",
            },
            { name: "evidence_reference", label: "对账依据" },
        ],
    },
}
function seconds(value: string) {
    return Math.floor(Date.parse(value) / 1000)
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
    const mutation = useDepositCommand()
    const defaults: Record<string, string> = Object.fromEntries(
        spec.fields.map((field) => [
            field.name,
            field.kind === "time" &&
            !["valid_until", "covered_from"].includes(field.name)
                ? format(new Date(), "yyyy-MM-dd'T'HH:mm")
                : "",
        ]),
    )
    const form = useAppForm({
        defaultValues: defaults,
        validators: { onSubmit: schema(spec) },
        onSubmit: async ({ value }) => {
            try {
                const command =
                    frozen.current ??
                    commandFor(
                        action,
                        value,
                        operation,
                        account,
                        request,
                        authorization,
                    )
                frozen.current = command
                await mutation.mutateAsync(command)
                onOpenChange(false)
            } catch (error) {
                const uncertain = isUncertainDepositCommandError(error)
                setUnknown(uncertain)
                if (!uncertain) frozen.current = null
                applyApiFormError(form, error)
            }
        },
    })
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
    return (
        <Dialog
            open={open}
            onOpenChange={(value) => {
                if (!mutation.isPending) onOpenChange(value)
            }}
        >
            <DialogContent
                closeButtonId={`supplier-deposits-${action}-close`}
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>{spec.title}</DialogTitle>
                    <DialogDescription>{spec.description}</DialogDescription>
                </DialogHeader>
                {account && (
                    <p className="text-body-compact">
                        {account.supplier_name} · {account.payer_party_name}
                        {request ? (
                            <>
                                {" "}
                                · 申请金额 <strong>{request.amount} 元</strong>
                            </>
                        ) : null}
                    </p>
                )}
                {unknown && (
                    <Alert>
                        <AlertDescription>
                            本次保存结果尚未查明。请刷新账户核对原结果；再次提交将查询或恢复原申请，当前资料已锁定。
                        </AlertDescription>
                    </Alert>
                )}
                <form
                    className="flex max-h-[65vh] flex-col gap-4 overflow-y-auto"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {spec.fields.map((specField) => (
                        <form.AppField
                            key={specField.name}
                            name={specField.name}
                        >
                            {(field) => {
                                const id = `supplier-deposits-${action}-${specField.name.replaceAll("_", "-")}`
                                if (
                                    specField.kind === "party" ||
                                    specField.kind === "supplier"
                                )
                                    return (
                                        <Field>
                                            <FieldLabel htmlFor={id}>
                                                {specField.label}
                                            </FieldLabel>
                                            {specField.kind === "party" ? (
                                                <SettlementPartySearchCombobox
                                                    id={id}
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(value) =>
                                                        field.handleChange(
                                                            value ?? "",
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
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(value) =>
                                                        field.handleChange(
                                                            value ?? "",
                                                        )
                                                    }
                                                    disabled={
                                                        unknown ||
                                                        mutation.isPending
                                                    }
                                                />
                                            )}
                                            {field.state.meta.errors.length ? (
                                                <FieldError
                                                    errors={field.state.meta.errors.map(
                                                        (error) => ({
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
                                if (specField.kind === "multiline")
                                    return (
                                        <field.TextareaField
                                            id={id}
                                            label={specField.label}
                                            description={specField.description}
                                            disabled={
                                                unknown || mutation.isPending
                                            }
                                            required={!specField.optional}
                                        />
                                    )
                                return (
                                    <field.TextField
                                        id={id}
                                        label={specField.label}
                                        type={
                                            specField.kind === "time"
                                                ? "datetime-local"
                                                : "text"
                                        }
                                        money={specField.kind === "amount"}
                                        inputMode={
                                            specField.kind === "amount"
                                                ? "decimal"
                                                : undefined
                                        }
                                        description={specField.description}
                                        disabled={unknown || mutation.isPending}
                                        required={!specField.optional}
                                    />
                                )
                            }}
                        </form.AppField>
                    ))}
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
                                label={unknown ? "查询原保存结果" : spec.submit}
                                loading={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
