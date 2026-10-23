"use client"

import * as React from "react"
import { OrganizationAccessImpact } from "./organization-access-impact"
import { z } from "zod"

import { BatchImpactPreview, BusinessDiffPanel } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { getErrorPresentation } from "@/lib/api/errors"
import {
    buildOrganizationChangeRequest,
    EMPTY_CHANGE_DRAFT,
    newIdempotencyKey,
    sameOrganizationChangeRequest,
    type OrganizationChangeDraft,
} from "@/features/organization/lib/change-payload"
import { impactChanges, impactCounts } from "@/features/organization/lib/impact"
import {
    KIND_LABEL,
    OPERATION_LABEL,
    ORGANIZATION_BOUNDARY_NOTICE,
} from "@/features/organization/lib/labels"
import { unitLabel } from "@/features/organization/lib/tree"
import type {
    OrganizationChangeReceipt,
    OrganizationStateView,
} from "@/features/organization/types"

const schema = z
    .object({
        operation: z.enum([
            "create_unit",
            "move_unit",
            "rename_unit",
            "set_leader",
            "disable_unit",
            "transfer_member",
            "end_membership",
            "grant_management",
            "revoke_management",
        ]),
        name: z.string(),
        parentId: z.string(),
        kind: z.enum(["department", "team"]),
        orgUnitId: z.string(),
        userId: z.string(),
        userIds: z.array(z.string()),
        roleId: z.string(),
        includeDescendants: z.enum(["true", "false"]),
        validTo: z.string(),
        assignmentId: z.string(),
        reason: z.string().trim().min(1, "必须填写变更原因").max(1000),
    })
    .superRefine((draft, context) => {
        if (draft.operation === "end_membership" && !draft.userId)
            context.addIssue({
                code: "custom",
                path: ["userId"],
                message: "请选择人员",
            })
        if (
            draft.operation === "transfer_member" &&
            !draft.userId &&
            draft.userIds.length === 0
        )
            context.addIssue({
                code: "custom",
                path: ["userIds"],
                message: "请选择人员",
            })
        if (draft.operation === "transfer_member" && !draft.orgUnitId)
            context.addIssue({
                code: "custom",
                path: ["orgUnitId"],
                message: "请选择部门",
            })
        if (["grant_management", "revoke_management"].includes(draft.operation))
            context.addIssue({
                code: "custom",
                path: ["operation"],
                message: "部门管理关系已停用，请在人员资料中设置数据范围。",
            })
    })

function PreviewReceiptGuard({
    values,
    expectedVersion,
    idempotencyKey,
    receipt,
    onInvalidate,
}: {
    values: OrganizationChangeDraft
    expectedVersion: number | undefined
    idempotencyKey: string
    receipt: OrganizationChangeReceipt | null
    onInvalidate: () => void
}) {
    React.useEffect(() => {
        if (!receipt) return
        if (expectedVersion === undefined) {
            onInvalidate()
            return
        }
        const request = buildOrganizationChangeRequest(
            expectedVersion,
            idempotencyKey,
            values,
        )
        if (!sameOrganizationChangeRequest(receipt.request, request)) {
            onInvalidate()
        }
    }, [values, expectedVersion, idempotencyKey, receipt, onInvalidate])
    return null
}

export function OrganizationChangeDialog({
    open,
    onOpenChange,
    view,
    draft,
    expectedVersion,
    blockedReason,
    previewing,
    onRefreshOptions,
    submitting,
    onPreview,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    view: OrganizationStateView | undefined
    draft: OrganizationChangeDraft
    expectedVersion: number | undefined
    blockedReason?: string
    previewing: boolean
    onRefreshOptions: () => Promise<unknown>
    submitting: boolean
    onPreview: (
        request: ReturnType<typeof buildOrganizationChangeRequest>,
    ) => Promise<OrganizationChangeReceipt>
    onSubmit: (
        request: ReturnType<typeof buildOrganizationChangeRequest>,
    ) => Promise<void>
}) {
    const [idempotencyKey, setIdempotencyKey] =
        React.useState(newIdempotencyKey)
    const [receipt, setReceipt] =
        React.useState<OrganizationChangeReceipt | null>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)

    const form = useAppForm({
        defaultValues: { ...EMPTY_CHANGE_DRAFT, ...draft },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (blockedReason || expectedVersion === undefined) return
            setActionError(null)
            const request = buildOrganizationChangeRequest(
                expectedVersion,
                idempotencyKey,
                value,
            )
            try {
                // 新建组织和批量添加成员只做组织归属调整，不进入影响预览。
                if (
                    value.operation === "create_unit" ||
                    (value.operation === "transfer_member" && !value.userId)
                ) {
                    await onSubmit(request)
                    onOpenChange(false)
                    return
                }
                if (
                    !receipt ||
                    !sameOrganizationChangeRequest(receipt.request, request)
                ) {
                    setReceipt(await onPreview(request))
                    return
                }
                await onSubmit(receipt.request)
                onOpenChange(false)
            } catch (error) {
                setActionError(getErrorPresentation(error).description)
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        setIdempotencyKey(newIdempotencyKey())
        setReceipt(null)
        setActionError(null)
        form.reset({ ...EMPTY_CHANGE_DRAFT, ...draft })
    }, [open, draft, form])

    const units = view?.units ?? []
    const people = view?.people ?? []
    const unitOptions = units
        .filter((unit) => unit.enabled)
        .map((unit) => ({ value: unit.id, label: unit.name }))
    const peopleById = new Map(people.map((person) => [person.id, person]))
    const peopleOptions = people
        .filter((person) => person.active)
        .map((person) => ({
            value: person.id,
            label: `${person.label}（${person.account}） · ${person.own_org_unit_id ? unitLabel(units, person.own_org_unit_id) : "未分配部门"}`,
        }))
    const peopleOptionsFor = (targetUnitId: string) =>
        people
            .filter((person) => person.active)
            .map((person) => {
                const alreadyThere =
                    Boolean(targetUnitId) &&
                    person.own_org_unit_id === targetUnitId
                return {
                    value: person.id,
                    label: `${person.label}（${person.account}） · ${
                        person.own_org_unit_id
                            ? unitLabel(units, person.own_org_unit_id)
                            : "未分配部门"
                    }${alreadyThere ? "（已在该部门）" : ""}`,
                    keywords: `${person.label} ${person.account}`,
                    disabled: alreadyThere,
                }
            })
    const changes = receipt ? impactChanges(receipt, view) : []
    const counts = impactCounts(changes)
    const skipsPreview =
        draft.operation === "create_unit" ||
        (draft.operation === "transfer_member" && !draft.userId)

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90vh] w-[calc(100vw-1.5rem)] max-w-2xl overflow-x-hidden overflow-y-auto sm:max-w-2xl"
                closeButtonId="organization-change-dialog-close"
                onKeyDownCapture={(event) => {
                    if (
                        event.key !== "Escape" ||
                        event.nativeEvent.isComposing ||
                        event.defaultPrevented
                    ) {
                        return
                    }
                    const target = event.target
                    if (
                        !(target instanceof Element) ||
                        !event.currentTarget.contains(target) ||
                        target.closest('[role="dialog"]') !==
                            event.currentTarget ||
                        event.currentTarget.querySelector(
                            '[aria-expanded="true"]',
                        )
                    ) {
                        return
                    }
                    // 收起状态的选择框会消费 Esc 清空选项；此时应关闭弹窗。
                    event.preventDefault()
                    event.stopPropagation()
                    onOpenChange(false)
                }}
            >
                <DialogHeader>
                    <DialogTitle>
                        {draft.operation === "transfer_member"
                            ? draft.userId
                                ? "调整所属部门"
                                : "添加部门成员"
                            : OPERATION_LABEL[draft.operation]}
                    </DialogTitle>
                    <DialogDescription>
                        {ORGANIZATION_BOUNDARY_NOTICE}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="min-w-0 space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {blockedReason ? (
                        <Alert>
                            <AlertTitle>录入内容已保留</AlertTitle>
                            <AlertDescription>{blockedReason}</AlertDescription>
                        </Alert>
                    ) : null}
                    <form.Subscribe
                        selector={(state) => state.values.operation}
                        children={(operation) => (
                            <>
                                {operation === "create_unit" ||
                                operation === "rename_unit" ? (
                                    <form.AppField
                                        name="name"
                                        children={(field) => (
                                            <field.TextField
                                                id="organization-change-name"
                                                label="组织名称"
                                                required
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "create_unit" ? (
                                    <form.AppField
                                        name="kind"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-kind"
                                                label="类型"
                                                required
                                                options={Object.entries(
                                                    KIND_LABEL,
                                                ).map(([value, label]) => ({
                                                    value,
                                                    label,
                                                }))}
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "create_unit" ||
                                operation === "move_unit" ? (
                                    <form.AppField
                                        name="parentId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-parent"
                                                label="上级组织"
                                                onRefresh={onRefreshOptions}
                                                options={unitOptions}
                                                allowClear
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "move_unit" ||
                                operation === "rename_unit" ||
                                operation === "set_leader" ||
                                operation === "disable_unit" ||
                                operation === "transfer_member" ? (
                                    <form.AppField
                                        name="orgUnitId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-unit"
                                                required
                                                label={
                                                    operation ===
                                                    "transfer_member"
                                                        ? "所属部门（调整后）"
                                                        : "目标组织"
                                                }
                                                onRefresh={onRefreshOptions}
                                                options={unitOptions}
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "transfer_member" &&
                                !draft.userId ? (
                                    <form.Subscribe
                                        selector={(state) =>
                                            state.values.orgUnitId
                                        }
                                    >
                                        {(targetUnitId) => (
                                            <form.AppField
                                                name="userIds"
                                                children={(field) => (
                                                    <field.MultiSelectField
                                                        id="organization-change-users"
                                                        label="人员"
                                                        required
                                                        placeholder="请选择人员，可多选"
                                                        onRefresh={
                                                            onRefreshOptions
                                                        }
                                                        options={peopleOptionsFor(
                                                            targetUnitId,
                                                        )}
                                                        getChipLabel={(
                                                            option,
                                                        ) =>
                                                            peopleById.get(
                                                                option.value,
                                                            )?.label ??
                                                            option.label
                                                        }
                                                    />
                                                )}
                                            />
                                        )}
                                    </form.Subscribe>
                                ) : null}
                                {operation === "set_leader" ? (
                                    <form.AppField
                                        name="userId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-leader"
                                                label="负责人"
                                                options={peopleOptions}
                                                placeholder="未设置负责人"
                                                description="清除选择后提交，可移除当前负责人。"
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "end_membership" ||
                                (operation === "transfer_member" &&
                                    Boolean(draft.userId)) ? (
                                    <form.AppField
                                        name="userId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-user"
                                                label="人员"
                                                required
                                                disabled={Boolean(draft.userId)}
                                                onRefresh={onRefreshOptions}
                                                options={peopleOptions}
                                            />
                                        )}
                                    />
                                ) : null}
                            </>
                        )}
                    />
                    {draft.operation === "transfer_member" ? (
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => {
                                const selectedPeople = (
                                    draft.userId
                                        ? [values.userId]
                                        : values.userIds
                                ).flatMap((id) => {
                                    const person = peopleById.get(id)
                                    return person ? [person] : []
                                })
                                const target = values.orgUnitId
                                    ? unitLabel(units, values.orgUnitId)
                                    : "请选择部门"
                                const summary =
                                    selectedPeople.length === 0
                                        ? "选择人员后，将显示当前部门与调整后的部门。"
                                        : selectedPeople.length === 1
                                          ? `${selectedPeople[0].label}：${
                                                selectedPeople[0]
                                                    .own_org_unit_id
                                                    ? unitLabel(
                                                          units,
                                                          selectedPeople[0]
                                                              .own_org_unit_id,
                                                      )
                                                    : "未分配部门"
                                            } → ${target}`
                                          : `已选择 ${selectedPeople.length} 人：${selectedPeople
                                                .map((person) => person.label)
                                                .join("、")} → ${target}`
                                return (
                                    <p
                                        className="rounded-lg bg-muted/40 p-3 text-sm"
                                        role="status"
                                    >
                                        {summary}
                                        <span className="mt-1 block text-xs text-muted-foreground">
                                            部门调整不会自动分配角色，也不会交接客户、单据或待办。
                                        </span>
                                    </p>
                                )
                            }}
                        </form.Subscribe>
                    ) : null}
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="organization-change-reason"
                                label="变更原因"
                                required
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.values}
                        children={(values) => (
                            <PreviewReceiptGuard
                                values={values}
                                expectedVersion={expectedVersion}
                                idempotencyKey={idempotencyKey}
                                receipt={receipt}
                                onInvalidate={() => setReceipt(null)}
                            />
                        )}
                    />

                    {skipsPreview || !view || blockedReason ? null : (
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => (
                                <OrganizationAccessImpact
                                    draft={values}
                                    view={view}
                                />
                            )}
                        </form.Subscribe>
                    )}

                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未完成</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}

                    {receipt && view && !blockedReason ? (
                        <div className="min-w-0 space-y-3">
                            <BatchImpactPreview
                                title={
                                    OPERATION_LABEL[
                                        receipt.request.change.operation
                                    ]
                                }
                                description={ORGANIZATION_BOUNDARY_NOTICE}
                                filterSummary="本次配置变更"
                                selectionScope="当前组织配置边界"
                                estimated={counts.estimated}
                                processable={counts.processable}
                                skipped={counts.skipped}
                                background={false}
                            />
                            <BusinessDiffPanel
                                title="变更前后"
                                caption="请核对调整内容，确认后生效"
                                changes={changes}
                            />
                        </div>
                    ) : null}

                    <DialogFooter className="flex min-w-0 flex-wrap justify-end gap-2">
                        <Button
                            id="organization-change-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            loading={previewing || submitting}
                            id="organization-change-submit"
                            type="submit"
                            disabled={
                                previewing ||
                                submitting ||
                                Boolean(blockedReason) ||
                                expectedVersion === undefined
                            }
                        >
                            {draft.operation === "create_unit"
                                ? "创建组织"
                                : draft.operation === "transfer_member" &&
                                    !draft.userId
                                  ? "添加成员"
                                  : receipt
                                    ? "确认提交"
                                    : "预览影响"}
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
