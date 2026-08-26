"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldGroup } from "@/components/ui/field"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FULFILLMENT_ROLES } from "@/features/fulfillment-operations/lib/fulfillment-roles"
import { FulfillmentOperationsWorkspace } from "@/features/fulfillment-operations/pages/components/fulfillment-operations-workspace"
import { FulfillmentPageStates } from "@/features/fulfillment-operations/pages/components/fulfillment-page-states"
import { useFulfillmentOperationsController } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller"
import {
    useWorkItemReassignCandidatesQuery,
    useWorkItemResponsibilityMutation,
} from "@/features/work-items/queries"
import { getErrorMessage } from "@/lib/api/errors"

import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

type WorkspaceFulfillmentTaskProps = Readonly<{
    item: WorkspaceWorkItem
    grantedPermissions: readonly string[]
    onTaskCompleted: (workItemId: string) => void
}>

const fulfillmentReassignSchema = z.object({
    targetUserId: z.string().min(1, "请选择新责任人"),
    reason: z
        .string()
        .max(150, "转交原因最多 150 个字符")
        .refine((value) => value.trim().length > 0, "请填写转交原因"),
})

/** W01 履约作业面：任务身份锁定一个强类型收货或发货操作。 */
export function WorkspaceFulfillmentTask({
    item,
    grantedPermissions,
    onTaskCompleted,
}: WorkspaceFulfillmentTaskProps) {
    const [reassignOpen, setReassignOpen] = React.useState(false)
    const descriptor = workspaceFulfillmentDescriptor(item)
    const operationType = descriptor?.operationTypes[0]
    const filters = React.useMemo<FulfillmentQueueFilters>(
        () => ({
            role: descriptor?.role ?? "sales_order",
            operationTypes: operationType ? [operationType] : [],
            operationId: item.businessObjectId,
            currentOperationId: item.businessObjectId,
        }),
        [descriptor?.role, item.businessObjectId, operationType],
    )
    const controller = useFulfillmentOperationsController({
        roleValue: filters.role,
        filters,
        lane: null,
        autoNextExplicit: "0",
        stateMode: "local",
        grantedPermissions,
        permissionsReady: true,
        executionAuthorized: item.allowedActions.includes("PROCESS"),
        onOperationCompleted: (operationId) => {
            if (operationId === item.businessObjectId) {
                onTaskCompleted(item.workItemId)
            }
        },
    })

    return (
        <section
            className="flex h-full min-h-0 flex-col"
            aria-label="当前履约任务"
        >
            <header className="flex shrink-0 items-start justify-between gap-3 border-b border-grid py-5">
                <div className="flex min-w-0 flex-col gap-2">
                    <WorkspaceDocumentBadge item={item} />
                    <h2 className="text-xl font-semibold tracking-tight">
                        {item.objectTitle}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        {item.ownerRoleLabel} · {item.ownerUserLabel}
                    </p>
                </div>
                {item.allowedActions.includes("REASSIGN") ? (
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => setReassignOpen(true)}
                    >
                        转交责任
                    </Button>
                ) : null}
            </header>

            <div className="min-h-0 flex-1 overflow-auto py-4">
                {!descriptor ? (
                    <Alert variant="destructive">
                        <AlertTitle>任务责任与履约对象不一致</AlertTitle>
                        <AlertDescription>
                            请联系管理员核对责任人、对象类型与任务原因后重试。
                        </AlertDescription>
                    </Alert>
                ) : controller.queueQuery.isPending ||
                  controller.queueQuery.isError ? (
                    <FulfillmentPageStates
                        status={
                            controller.queueQuery.isPending
                                ? "pending"
                                : "error"
                        }
                        standalone
                        embedded
                        headerDescription="履约处理"
                        error={controller.queueQuery.error}
                        onRetry={() => void controller.queueQuery.refetch()}
                    />
                ) : (
                    <FulfillmentOperationsWorkspace
                        controller={controller}
                        headerDescription="当前任务"
                        operationTypes={[operationType!]}
                        roleLabel={
                            controller.context?.roleLabel ??
                            FULFILLMENT_ROLES[descriptor.role].label
                        }
                        embedded
                        singleOperation
                        onBack={() => undefined}
                    />
                )}
            </div>
            <WorkspaceFulfillmentReassignDialog
                open={reassignOpen}
                onOpenChange={setReassignOpen}
                item={item}
                onReassigned={() => onTaskCompleted(item.workItemId)}
            />
        </section>
    )
}

function WorkspaceFulfillmentReassignDialog({
    open,
    onOpenChange,
    item,
    onReassigned,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    item: WorkspaceWorkItem
    onReassigned: () => void
}) {
    const [error, setError] = React.useState<string | null>(null)
    const intentRef = React.useRef<{
        fingerprint: string
        idempotencyKey: string
    } | null>(null)
    const candidatesQuery = useWorkItemReassignCandidatesQuery(
        item.workItemId,
        open,
    )
    const mutation = useWorkItemResponsibilityMutation()

    const options = React.useMemo(
        () =>
            (candidatesQuery.data ?? []).map((candidate) => ({
                value: candidate.user_id,
                label: `${candidate.display_name} · ${candidate.account}`,
                keywords: candidate.account,
            })),
        [candidatesQuery.data],
    )
    const form = useAppForm({
        defaultValues: {
            targetUserId: "",
            reason: "",
        },
        validators: {
            onChange: fulfillmentReassignSchema,
        },
        onSubmit: async ({ value }) => {
            const normalizedReason = value.reason.trim()
            const selectedIsEligible = options.some(
                (option) => option.value === value.targetUserId,
            )
            if (!selectedIsEligible || !normalizedReason) {
                setError("请选择当前合格人员，并填写转交原因")
                return
            }
            const fingerprint = JSON.stringify({
                workItemId: item.workItemId,
                taskVersion: item.taskVersion,
                targetUserId: value.targetUserId,
                reason: normalizedReason,
            })
            if (intentRef.current?.fingerprint !== fingerprint) {
                intentRef.current = {
                    fingerprint,
                    idempotencyKey: `work-item-reassign:${item.workItemId}:${crypto.randomUUID()}`,
                }
            }
            try {
                await mutation.mutateAsync({
                    kind: "REASSIGN",
                    workItemId: item.workItemId,
                    expectedTaskVersion: item.taskVersion,
                    targetUserId: value.targetUserId,
                    reason: normalizedReason,
                    idempotencyKey: intentRef.current.idempotencyKey,
                })
                intentRef.current = null
                onOpenChange(false)
                onReassigned()
            } catch (cause) {
                setError(getErrorMessage(cause, "任务责任未转交，请刷新后重试"))
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        form.reset({ targetUserId: "", reason: "" })
        setError(null)
        intentRef.current = null
    }, [form, item.workItemId, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>转交履约责任</DialogTitle>
                    <DialogDescription>
                        当前责任人：{item.ownerUserLabel}
                        。采购单责任转交会同步更新该采购单全部开放交付任务；历史任务不变。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {candidatesQuery.isError ? (
                        <Alert variant="destructive">
                            <AlertTitle>候选人员加载失败</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(
                                    candidatesQuery.error,
                                    "请关闭后重试",
                                )}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {error ? (
                        <Alert variant="destructive">
                            <AlertTitle>没有转交</AlertTitle>
                            <AlertDescription>{error}</AlertDescription>
                        </Alert>
                    ) : null}
                    <FieldGroup>
                        <form.AppField name="targetUserId">
                            {(field) => (
                                <field.SelectField
                                    label="新责任人"
                                    options={options}
                                    placeholder="选择合格人员"
                                    emptyLabel="没有同时满足当前责任约束的人员"
                                    description="最终提交时会再次校验账号状态、完整操作权限与全部开放任务。"
                                    loading={candidatesQuery.isPending}
                                    disabled={candidatesQuery.isError}
                                    allowClear={false}
                                    required
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="reason">
                            {(field) => (
                                <field.TextareaField
                                    label="转交原因"
                                    placeholder="说明本次责任调整依据"
                                    maxLength={150}
                                />
                            )}
                        </form.AppField>
                    </FieldGroup>
                    <DialogFooter>
                        <DialogClose
                            render={<Button type="button" variant="outline" />}
                        >
                            取消
                        </DialogClose>
                        <form.Subscribe
                            selector={(state) => state.values.targetUserId}
                        >
                            {(targetUserId) => (
                                <form.AppForm>
                                    <form.SubmitButton
                                        label="确认转交"
                                        pendingLabel="转交中…"
                                        disabled={
                                            mutation.isPending ||
                                            candidatesQuery.isPending ||
                                            candidatesQuery.isError ||
                                            !options.some(
                                                (option) =>
                                                    option.value ===
                                                    targetUserId,
                                            )
                                        }
                                    />
                                </form.AppForm>
                            )}
                        </form.Subscribe>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
