"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { FieldGroup } from "@/components/ui/field"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import { mapDocumentApprovalViewDto } from "@/features/approval-workflow/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import {
    useFinancialDraftMutations,
    useFinancialDraftQuery,
} from "../hooks/queries"
import {
    FINANCIAL_DRAFTS,
    financialDraftFormValues,
    isFinancialDraft,
    isFinancialDraftEditor,
    parseFinancialDraftKind,
    receivedAtUnixSeconds,
    type FinancialDraft,
    type FinancialDraftFormValues,
    type FinancialDraftKind,
    type FinancialDraftSide,
} from "../types"
import {
    allocationTotalWithinAmount,
    matchesSavedFields,
    submitAllocations,
    type FinancialSaveIntent,
    type FinancialSubmitIntent,
    type FinancialUnknownIntent,
} from "../lib/edit-intent"

function positiveAmount(value: string) {
    try {
        return compareDecimal(value.trim(), "0", 2) > 0
    } catch {
        return false
    }
}

/** 客户及供应商往来页的原单编辑深链窗口。 */
export function FinancialDraftEditDialog({
    side,
}: {
    side: FinancialDraftSide
}) {
    const searchParams = useSearchParams()
    const pathname = usePathname()
    const router = useRouter()
    const kind = parseFinancialDraftKind(searchParams.get("editType"), side)
    const id = searchParams.get("editId")?.trim()
    if (!kind || !id) return null

    const finish = () => {
        const params = new URLSearchParams(searchParams.toString())
        params.delete("editId")
        params.delete("editType")
        params.set("previewKind", FINANCIAL_DRAFTS[kind].preview)
        params.set(side === "customer" ? "previewId" : "detailId", id)
        router.replace(`${pathname}?${params}`, { scroll: false })
    }

    return (
        <FinancialDraftEditor
            key={`${kind}:${id}`}
            kind={kind}
            documentId={id}
            onClose={finish}
        />
    )
}

function FinancialDraftEditor({
    kind,
    documentId,
    onClose,
}: {
    kind: FinancialDraftKind
    documentId: string
    onClose: () => void
}) {
    const query = useFinancialDraftQuery(kind, documentId)
    const [busy, setBusy] = React.useState(false)
    const id = `financial-draft-edit-${kind.replaceAll("_", "-")}-${toAutomationIdSegment(documentId)}`

    return (
        <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!busy}
                className="max-h-[85dvh] overflow-y-auto sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        修改{FINANCIAL_DRAFTS[kind].label}
                    </DialogTitle>
                    <DialogDescription>
                        保留原单号与来源，保存修改后重新提交审批。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p className="text-sm text-muted-foreground">
                        正在读取原单…
                    </p>
                ) : query.isError && !query.data ? (
                    <Alert variant="destructive">
                        <AlertTitle>原单读取失败</AlertTitle>
                        <AlertDescription>
                            {getErrorMessage(query.error)}
                            <Button
                                id={`${id}-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                className="mt-3"
                                onClick={() => void query.refetch()}
                            >
                                重新读取
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : query.data ? (
                    <FinancialDraftForm
                        kind={kind}
                        draft={query.data}
                        id={id}
                        onClose={onClose}
                        onBusyChange={setBusy}
                        onRefresh={async () => {
                            const result = await query.refetch()
                            if (result.isError) throw result.error
                            return result.data
                        }}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

/** 只编辑该类型公开的草稿字段；提交始终消费保存后返回的版本。 */
function FinancialDraftForm({
    kind,
    draft,
    id,
    onClose,
    onRefresh,
    onBusyChange,
}: {
    kind: FinancialDraftKind
    draft: FinancialDraft
    id: string
    onClose: () => void
    onRefresh: () => Promise<FinancialDraft | undefined>
    onBusyChange: (busy: boolean) => void
}) {
    const profileQuery = useAccountProfileQuery()
    const mutations = useFinancialDraftMutations(kind, draft.id)
    const approval = draft.approval
        ? mapDocumentApprovalViewDto(draft.approval)
        : undefined
    const editable = isFinancialDraft(draft)
    const canSave =
        editable &&
        isFinancialDraftEditor(kind, draft, profileQuery.data?.userid) &&
        hasPermission(profileQuery.data?.permissions, `${kind}:submit`)
    const canSubmit =
        canSave &&
        hasPermission(profileQuery.data?.permissions, `${kind}:submit`) &&
        Boolean(approval?.allowedActions.includes("SUBMIT")) &&
        (kind !== "customer_receipt" ||
            Boolean(draft.pending_allocations?.length))
    const [error, setError] = React.useState<string | null>(null)
    const [saved, setSaved] = React.useState(false)
    const [unknown, setUnknown] = React.useState<FinancialUnknownIntent | null>(
        null,
    )
    const [checking, setChecking] = React.useState(false)
    const submitAfterSave = React.useRef(true)
    const submissionInFlight = React.useRef(false)
    const commandInFlight = React.useRef(false)
    const pending = mutations.save.isPending || mutations.submit.isPending
    const defaultValues = React.useMemo(
        () => financialDraftFormValues(draft),
        [draft],
    )
    const appliedDefaults = React.useRef(defaultValues)
    const schema = React.useMemo(
        () =>
            z
                .object({
                    amount: z
                        .string()
                        .refine(positiveAmount, "金额须大于 0，最多两位小数"),
                    receivedAt: z
                        .string()
                        .refine(
                            (value) =>
                                kind !== "customer_receipt" ||
                                receivedAtUnixSeconds(value) !== null,
                            "请选择到账时间",
                        ),
                    bankReference: z
                        .string()
                        .max(256, "银行流水号最多 256 个字符"),
                    reasonText: z
                        .string()
                        .refine(
                            (value) =>
                                kind === "customer_receipt" ||
                                value.trim().length > 0,
                            "请填写原因",
                        ),
                    allocations: z.array(
                        z.object({
                            receivableEntryId: z.string().min(1),
                            allocatedAmount: z
                                .string()
                                .refine(
                                    positiveAmount,
                                    "核销金额须大于 0，最多两位小数",
                                ),
                        }),
                    ),
                })
                .superRefine((values, context) => {
                    if (
                        kind === "customer_receipt" &&
                        !allocationTotalWithinAmount(values)
                    ) {
                        context.addIssue({
                            code: "custom",
                            path: ["amount"],
                            message:
                                "核销合计不得大于到账金额，请同时调整原核销分配",
                        })
                    }
                }),
        [kind],
    )
    const form = useAppForm({
        defaultValues,
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (!canSave || pending || unknown || submissionInFlight.current)
                return
            submissionInFlight.current = true
            const shouldSubmit = submitAfterSave.current
            setError(null)
            setSaved(false)
            try {
                await saveIntent({
                    stage: "save",
                    version: draft.version,
                    values: structuredClone(value),
                    shouldSubmit,
                })
            } finally {
                submissionInFlight.current = false
            }
        },
    })
    const submitIntent = async (intent: FinancialSubmitIntent) => {
        if (commandInFlight.current) return
        commandInFlight.current = true
        try {
            await mutations.submit.mutateAsync({
                kind,
                id: draft.id,
                version: intent.version,
                idempotencyKey: intent.idempotencyKey,
                allocations: intent.allocations,
            })
            setUnknown(null)
            onClose()
        } catch (cause) {
            setUnknown(
                classifyFormalCommandError(cause) === "unknown" ||
                    unknown?.stage === "submit"
                    ? intent
                    : null,
            )
            setError(getErrorMessage(cause, "修改已保存，提交未完成，请重试。"))
        } finally {
            commandInFlight.current = false
        }
    }
    const newSubmitIntent = (
        version: number,
        values: FinancialDraftFormValues,
    ): FinancialSubmitIntent => ({
        stage: "submit",
        version,
        idempotencyKey: `financial-draft-${kind}-${draft.id}-${crypto.randomUUID()}`,
        allocations:
            kind === "customer_receipt" ? submitAllocations(values) : undefined,
    })
    const saveIntent = async (intent: FinancialSaveIntent) => {
        if (commandInFlight.current) return
        commandInFlight.current = true
        let savedDraft: FinancialDraft
        try {
            savedDraft = await mutations.save.mutateAsync({
                kind,
                id: draft.id,
                version: intent.version,
                values: intent.values,
            })
        } catch (cause) {
            if (
                classifyFormalCommandError(cause) === "unknown" ||
                unknown?.stage === "save"
            )
                setUnknown({
                    ...intent,
                    confirmedUnchanged: false,
                    latest: undefined,
                })
            setError(getErrorMessage(cause, "修改未保存，请重试。"))
            return
        } finally {
            commandInFlight.current = false
        }
        setSaved(true)
        setUnknown(null)
        if (intent.shouldSubmit && canSubmit)
            await submitIntent(
                newSubmitIntent(savedDraft.version, intent.values),
            )
    }
    const resolveUnknown = async () => {
        if (!unknown || checking || pending) return
        setChecking(true)
        setError(null)
        try {
            const current = await onRefresh()
            if (!current || current.id !== draft.id)
                throw new Error("当前原单读取结果无效")
            if (unknown.stage === "submit") {
                if (!isFinancialDraft(current)) {
                    await mutations.confirm(current)
                    setUnknown(null)
                    onClose()
                } else
                    setError(
                        "原单仍为草稿，本次提交尚未确认。可使用原操作重试。",
                    )
            } else if (unknown.stage === "save") {
                if (
                    current.version > unknown.version &&
                    matchesSavedFields(kind, current, unknown.values)
                ) {
                    await mutations.confirm(current)
                    setSaved(true)
                    if (!isFinancialDraft(current)) {
                        setUnknown(null)
                        onClose()
                    } else
                        setUnknown({
                            stage: "saved",
                            version: current.version,
                            values: unknown.values,
                            shouldSubmit: unknown.shouldSubmit,
                        })
                } else if (
                    current.version === unknown.version &&
                    isFinancialDraft(current)
                ) {
                    setUnknown({ ...unknown, confirmedUnchanged: true })
                    setError(
                        "原单版本未变化，尚未确认保存完成。可使用原值重试保存。",
                    )
                } else {
                    setUnknown({ ...unknown, latest: current })
                    setError("原单状态或内容已变化，请采用最新原单后继续处理。")
                }
            }
        } catch (cause) {
            setError(getErrorMessage(cause, "暂时无法查询当前状态，请重试。"))
        } finally {
            setChecking(false)
        }
    }
    React.useEffect(() => {
        if (
            !pending &&
            !unknown &&
            !checking &&
            !submissionInFlight.current &&
            appliedDefaults.current !== defaultValues
        ) {
            appliedDefaults.current = defaultValues
            const edited = new Map(
                form.state.values.allocations.map((line) => [
                    line.receivableEntryId,
                    line.allocatedAmount,
                ]),
            )
            form.reset({
                ...defaultValues,
                allocations: defaultValues.allocations.map((line) => ({
                    ...line,
                    allocatedAmount:
                        edited.get(line.receivableEntryId) ??
                        line.allocatedAmount,
                })),
            })
        }
    }, [defaultValues, form, pending, unknown, checking])
    React.useEffect(() => {
        onBusyChange(pending || checking || Boolean(unknown))
    }, [onBusyChange, pending, unknown, checking])

    return (
        <form.AppForm>
            <form
                id={`${id}-form`}
                className="space-y-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <p className="num text-sm text-muted-foreground">
                    单号：
                    {draft.receipt_no ??
                        draft.refund_no ??
                        draft.reversal_no ??
                        "—"}
                </p>
                {!editable ? (
                    <Alert>
                        <AlertTitle>原单当前不可编辑</AlertTitle>
                        <AlertDescription>
                            当前单据已进入审批或已完成处理。请关闭窗口查看原单状态。
                        </AlertDescription>
                    </Alert>
                ) : !canSave ? (
                    <p className="text-sm text-muted-foreground">
                        {profileQuery.isPending
                            ? "正在核对操作权限…"
                            : "当前账号没有修改此原单的权限。"}
                    </p>
                ) : null}
                <FieldGroup>
                    <form.AppField name="amount">
                        {(field) => (
                            <field.TextField
                                id={`${id}-amount`}
                                label="金额"
                                inputMode="decimal"
                                required
                                disabled={
                                    !canSave || pending || Boolean(unknown)
                                }
                            />
                        )}
                    </form.AppField>
                    {kind === "customer_receipt" ? (
                        <>
                            <form.AppField name="receivedAt">
                                {(field) => (
                                    <field.DateTimeField
                                        id={`${id}-received-at`}
                                        label="到账时间"
                                        required
                                        clearable={false}
                                        disabled={
                                            !canSave ||
                                            pending ||
                                            Boolean(unknown)
                                        }
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="bankReference">
                                {(field) => (
                                    <field.TextField
                                        id={`${id}-bank-reference`}
                                        label="银行流水号"
                                        disabled={
                                            !canSave ||
                                            pending ||
                                            Boolean(unknown)
                                        }
                                    />
                                )}
                            </form.AppField>
                            {defaultValues.allocations.map(
                                (allocation, index) => (
                                    <form.AppField
                                        key={allocation.receivableEntryId}
                                        name={`allocations[${index}].allocatedAmount`}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                id={`${id}-allocation-${toAutomationIdSegment(allocation.receivableEntryId)}-amount`}
                                                label={
                                                    defaultValues.allocations
                                                        .length === 1
                                                        ? "原核销金额"
                                                        : `原核销金额 ${index + 1}`
                                                }
                                                description="沿用原应收来源，核销金额随本次提交保存"
                                                inputMode="decimal"
                                                required
                                                disabled={
                                                    !canSave ||
                                                    pending ||
                                                    Boolean(unknown)
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                ),
                            )}
                            {!defaultValues.allocations.length ? (
                                <p className="text-sm text-muted-foreground">
                                    原单未保存核销来源，当前只能保存到账信息。
                                </p>
                            ) : null}
                        </>
                    ) : (
                        <form.AppField name="reasonText">
                            {(field) => (
                                <field.TextareaField
                                    id={`${id}-reason`}
                                    label="原因说明"
                                    required
                                    disabled={
                                        !canSave || pending || Boolean(unknown)
                                    }
                                />
                            )}
                        </form.AppField>
                    )}
                </FieldGroup>
                {editable ? (
                    <SubmissionRouteConfirmation
                        definition={approval?.definition}
                    />
                ) : null}
                {saved && !error ? (
                    <p className="text-sm text-muted-foreground">
                        {kind === "customer_receipt"
                            ? "到账信息已保存，核销分配在提交审批时保存。"
                            : "修改已保存。"}
                    </p>
                ) : null}
                {error ? (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                ) : null}
                {unknown ? (
                    <Alert variant="warning">
                        <AlertTitle>
                            {unknown.stage === "saved"
                                ? "保存结果已确认"
                                : unknown.stage === "save"
                                  ? "保存结果待确认"
                                  : "提交结果待确认"}
                        </AlertTitle>
                        <AlertDescription>
                            本次内容已保留，确认前不能修改或关闭。
                        </AlertDescription>
                    </Alert>
                ) : null}
                {unknown ? (
                    <div className="flex flex-wrap gap-2">
                        {unknown.stage !== "saved" ? (
                            <LoadingButton
                                id={`${id}-resolve`}
                                type="button"
                                variant="outline"
                                loading={checking}
                                disabled={pending || checking}
                                onClick={() => void resolveUnknown()}
                            >
                                {unknown.stage === "save"
                                    ? "查询保存结果"
                                    : "查询提交结果"}
                            </LoadingButton>
                        ) : null}
                        {unknown.stage === "submit" ? (
                            <LoadingButton
                                id={`${id}-retry-submit`}
                                type="button"
                                variant="outline"
                                loading={mutations.submit.isPending}
                                disabled={pending || checking}
                                onClick={() => void submitIntent(unknown)}
                            >
                                重试提交审批
                            </LoadingButton>
                        ) : null}
                        {unknown.stage === "save" &&
                        unknown.confirmedUnchanged ? (
                            <LoadingButton
                                id={`${id}-retry-save`}
                                type="button"
                                variant="outline"
                                loading={mutations.save.isPending}
                                disabled={pending || checking}
                                onClick={() => void saveIntent(unknown)}
                            >
                                使用原值重试保存
                            </LoadingButton>
                        ) : null}
                        {unknown.stage === "save" && unknown.latest ? (
                            <Button
                                id={`${id}-adopt-latest`}
                                type="button"
                                variant="outline"
                                disabled={pending || checking}
                                onClick={async () => {
                                    if (
                                        unknown.stage !== "save" ||
                                        !unknown.latest
                                    )
                                        return
                                    await mutations.confirm(unknown.latest)
                                    appliedDefaults.current =
                                        financialDraftFormValues(unknown.latest)
                                    form.reset(appliedDefaults.current)
                                    setUnknown(null)
                                    setError(null)
                                }}
                            >
                                采用最新原单
                            </Button>
                        ) : null}
                        {unknown.stage === "saved" ? (
                            <>
                                <Button
                                    id={`${id}-continue-edit`}
                                    type="button"
                                    variant="outline"
                                    disabled={pending || checking}
                                    onClick={() => {
                                        appliedDefaults.current = defaultValues
                                        form.reset(unknown.values)
                                        setUnknown(null)
                                        setError(null)
                                    }}
                                >
                                    继续编辑
                                </Button>
                                {unknown.shouldSubmit && canSubmit ? (
                                    <LoadingButton
                                        id={`${id}-continue-submit`}
                                        type="button"
                                        loading={mutations.submit.isPending}
                                        disabled={pending || checking}
                                        onClick={() =>
                                            void submitIntent(
                                                newSubmitIntent(
                                                    unknown.version,
                                                    unknown.values,
                                                ),
                                            )
                                        }
                                    >
                                        继续提交审批
                                    </LoadingButton>
                                ) : null}
                            </>
                        ) : null}
                    </div>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${id}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={pending || Boolean(unknown)}
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                    <form.Subscribe
                        selector={(state) =>
                            [state.canSubmit, state.isSubmitting] as const
                        }
                    >
                        {([formCanSubmit, submitting]) => (
                            <>
                                <LoadingButton
                                    id={`${id}-save`}
                                    type="button"
                                    variant="outline"
                                    disabled={
                                        !canSave ||
                                        !formCanSubmit ||
                                        pending ||
                                        submitting ||
                                        Boolean(unknown)
                                    }
                                    loading={
                                        submitting && !submitAfterSave.current
                                    }
                                    onClick={() => {
                                        submitAfterSave.current = false
                                        void form.handleSubmit()
                                    }}
                                >
                                    保存修改
                                </LoadingButton>
                                <LoadingButton
                                    id={`${id}-submit`}
                                    type="submit"
                                    disabled={
                                        !canSubmit ||
                                        !formCanSubmit ||
                                        pending ||
                                        submitting ||
                                        Boolean(unknown)
                                    }
                                    loading={
                                        submitting && submitAfterSave.current
                                    }
                                    onClick={() => {
                                        submitAfterSave.current = true
                                    }}
                                >
                                    保存并提交审批
                                </LoadingButton>
                            </>
                        )}
                    </form.Subscribe>
                </DialogFooter>
            </form>
        </form.AppForm>
    )
}
