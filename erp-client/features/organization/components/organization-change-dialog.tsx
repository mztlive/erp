"use client"

import * as React from "react"
import { z } from "zod"

import { BatchImpactPreview, BusinessDiffPanel } from "@/components/business"
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
    MANAGEMENT_GRANT_NOTICE,
    OPERATION_LABEL,
    ORGANIZATION_BOUNDARY_NOTICE,
} from "@/features/organization/lib/labels"
import {
    isRelationActive,
    personLabel,
    roleLabel,
    unitLabel,
} from "@/features/organization/lib/tree"
import type {
    OrganizationChangeReceipt,
    OrganizationStateView,
} from "@/features/organization/types"

const schema = z.object({
    operation: z.enum([
        "create_unit",
        "move_unit",
        "rename_unit",
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
    roleId: z.string(),
    includeDescendants: z.enum(["true", "false"]),
    validTo: z.string(),
    assignmentId: z.string(),
    reason: z.string().trim().min(1, "必须填写变更原因").max(1000),
})

function PreviewReceiptGuard({
    values,
    expectedVersion,
    idempotencyKey,
    receipt,
    onInvalidate,
}: {
    values: OrganizationChangeDraft
    expectedVersion: number
    idempotencyKey: string
    receipt: OrganizationChangeReceipt | null
    onInvalidate: () => void
}) {
    React.useEffect(() => {
        if (!receipt) return
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
    previewing,
    submitting,
    onPreview,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    view: OrganizationStateView
    draft: OrganizationChangeDraft
    expectedVersion: number
    previewing: boolean
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
            setActionError(null)
            const request = buildOrganizationChangeRequest(
                expectedVersion,
                idempotencyKey,
                value,
            )
            try {
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

    const unitOptions = view.units
        .filter((unit) => unit.enabled)
        .map((unit) => ({ value: unit.id, label: unit.name }))
    const peopleOptions = view.people
        .filter((person) => person.active)
        .map((person) => ({
            value: person.id,
            label: `${person.label}（${person.account}）`,
        }))
    const roleOptions = view.roles
        .filter((role) => role.enabled)
        .map((role) => ({ value: role.id, label: role.name }))
    const assignmentOptions = view.management
        .filter((item) =>
            isRelationActive(item.valid_from, item.valid_to, view.asOf),
        )
        .map((item) => ({
            value: item.id,
            label: `${personLabel(view.people, item.user_id)} · ${roleLabel(view.roles, item.role_id)} · ${unitLabel(view.units, item.org_unit_id)}`,
        }))
    const changes = receipt ? impactChanges(receipt, view) : []
    const counts = impactCounts(changes)

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90vh] w-[calc(100vw-1.5rem)] max-w-2xl overflow-x-hidden overflow-y-auto sm:max-w-2xl"
                closeButtonId="organization-change-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>组织变更影响预览</DialogTitle>
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
                    <form.AppField
                        name="operation"
                        children={(field) => (
                            <field.SelectField
                                id="organization-change-operation"
                                label="变更类型"
                                options={Object.entries(OPERATION_LABEL).map(
                                    ([value, label]) => ({ value, label }),
                                )}
                            />
                        )}
                    />
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
                                                options={unitOptions}
                                                allowClear
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "move_unit" ||
                                operation === "rename_unit" ||
                                operation === "disable_unit" ||
                                operation === "transfer_member" ||
                                operation === "grant_management" ? (
                                    <form.AppField
                                        name="orgUnitId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-unit"
                                                label="目标组织"
                                                options={unitOptions}
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "transfer_member" ||
                                operation === "end_membership" ||
                                operation === "grant_management" ? (
                                    <form.AppField
                                        name="userId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-user"
                                                label="人员"
                                                options={peopleOptions}
                                            />
                                        )}
                                    />
                                ) : null}
                                {operation === "grant_management" ? (
                                    <>
                                        <p className="text-sm text-muted-foreground">
                                            {MANAGEMENT_GRANT_NOTICE}
                                        </p>
                                        <form.AppField
                                            name="roleId"
                                            children={(field) => (
                                                <field.SelectField
                                                    id="organization-change-role"
                                                    label="角色"
                                                    options={roleOptions}
                                                />
                                            )}
                                        />
                                        <form.AppField
                                            name="includeDescendants"
                                            children={(field) => (
                                                <field.SelectField
                                                    id="organization-change-descendants"
                                                    label="是否包含下级"
                                                    options={[
                                                        {
                                                            value: "false",
                                                            label: "仅本级",
                                                        },
                                                        {
                                                            value: "true",
                                                            label: "含下级",
                                                        },
                                                    ]}
                                                />
                                            )}
                                        />
                                        <form.AppField
                                            name="validTo"
                                            children={(field) => (
                                                <field.DateTimeField
                                                    id="organization-change-valid-to"
                                                    label="有效期结束（可选）"
                                                />
                                            )}
                                        />
                                    </>
                                ) : null}
                                {operation === "revoke_management" ? (
                                    <form.AppField
                                        name="assignmentId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-change-assignment"
                                                label="管理授权"
                                                options={assignmentOptions}
                                            />
                                        )}
                                    />
                                ) : null}
                            </>
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="organization-change-reason"
                                label="变更原因"
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

                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未完成</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}

                    {receipt ? (
                        <div className="min-w-0 space-y-3">
                            <BatchImpactPreview
                                title={
                                    OPERATION_LABEL[
                                        receipt.request.change.operation
                                    ]
                                }
                                description={ORGANIZATION_BOUNDARY_NOTICE}
                                filterSummary={`期望版本 ${receipt.request.expected_version}`}
                                selectionScope="当前组织配置边界"
                                estimated={counts.estimated}
                                processable={counts.processable}
                                skipped={counts.skipped}
                                background={false}
                            />
                            <BusinessDiffPanel
                                title="变更前后"
                                caption="提交时会再次核对版本、权限和未结业务"
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
                        <Button
                            id="organization-change-submit"
                            type="submit"
                            disabled={previewing || submitting}
                        >
                            {receipt ? "确认提交" : "预览影响"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
