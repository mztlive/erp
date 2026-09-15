"use client"

import * as React from "react"
import { z } from "zod"

import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
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
import { Field, FieldLabel } from "@/components/ui/field"
import {
    actionLabel,
    resourceLabel,
} from "@/features/admin/lib/permission-catalog"
import {
    DIMENSION_LABEL,
    SCOPE_TYPE_LABEL,
    TARGET_MODE_LABEL,
} from "@/features/organization/lib/labels"
import {
    registeredResources,
    validateCreateDataScope,
} from "@/features/organization/lib/scope-payload"
import type {
    CreateDataScopeInput,
    DataScopeType,
    OrganizationStateView,
    ScopeDimension,
    ScopeTargetMode,
} from "@/features/organization/types"
import { ScopeTargetPicker } from "./scope-target-picker"
import { getErrorPresentation } from "@/lib/api/errors"

const schema = z.object({
    subjectType: z.enum(["role", "user"]),
    subjectId: z.string().min(1, "请选择主体"),
    scopeType: z.enum([
        "company",
        "organization",
        "team",
        "self_owned",
        "collaborative",
    ]),
    resource: z.string().min(1, "请选择资源"),
    actions: z.array(z.string()).min(1, "请选择动作"),
    targetDimension: z.enum(["internal_org", "settlement_party", "warehouse"]),
    targetMode: z.enum(["explicit", "own_org", "managed_orgs", "none"]),
    includeDescendants: z.enum(["true", "false", "none"]),
    scopeTargets: z.array(z.string()),
})

type Draft = z.infer<typeof schema>

const DEFAULT_DRAFT: Draft = {
    subjectType: "role",
    subjectId: "",
    scopeType: "company",
    resource: "",
    actions: [],
    targetDimension: "internal_org",
    targetMode: "none",
    includeDescendants: "none",
    scopeTargets: [],
}

export function DataScopeFormDialog({
    open,
    onOpenChange,
    roles,
    people,
    units,
    submitting,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    roles: OrganizationStateView["roles"]
    people: OrganizationStateView["people"]
    units: OrganizationStateView["units"]
    submitting: boolean
    onSubmit: (input: CreateDataScopeInput) => Promise<void>
}) {
    const resources = React.useMemo(() => registeredResources(), [])
    const [actionError, setActionError] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: DEFAULT_DRAFT,
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            const needsTargets =
                value.scopeType === "organization" || value.scopeType === "team"
            const input: CreateDataScopeInput = {
                subjectType: value.subjectType,
                subjectId: value.subjectId,
                scopeType: value.scopeType as DataScopeType,
                resource: value.resource,
                actions: value.actions,
                targetDimension: value.targetDimension as ScopeDimension,
                targetMode: needsTargets
                    ? value.targetDimension === "internal_org"
                        ? (value.targetMode as ScopeTargetMode)
                        : "explicit"
                    : null,
                includeDescendants:
                    !needsTargets ||
                    value.targetDimension !== "internal_org" ||
                    value.targetMode === "managed_orgs" ||
                    value.includeDescendants === "none"
                        ? null
                        : value.includeDescendants === "true",
                scopeTargets: needsTargets ? value.scopeTargets : [],
            }
            const invalid = validateCreateDataScope(input)
            if (invalid) {
                setActionError(invalid)
                return
            }
            try {
                setActionError(null)
                await onSubmit(input)
                onOpenChange(false)
            } catch (error) {
                setActionError(getErrorPresentation(error).description)
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        setActionError(null)
        form.reset(DEFAULT_DRAFT)
    }, [open, form])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90vh] w-[calc(100vw-1.5rem)] max-w-xl overflow-x-hidden overflow-y-auto"
                closeButtonId="organization-scope-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>按资源与动作配置范围</DialogTitle>
                    <DialogDescription>
                        必须指定资源和动作。目标只能使用稳定
                        ID，不能用通配符或显示名。
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
                        name="subjectType"
                        children={(field) => (
                            <field.SelectField
                                id="organization-scope-subject-type"
                                label="主体类型"
                                options={[
                                    { value: "role", label: "角色" },
                                    { value: "user", label: "用户上限" },
                                ]}
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.values.subjectType}
                        children={(subjectType) => (
                            <form.AppField
                                name="subjectId"
                                children={(field) => (
                                    <field.SelectField
                                        id="organization-scope-subject"
                                        label="主体"
                                        options={
                                            subjectType === "user"
                                                ? people.map((person) => ({
                                                      value: person.id,
                                                      label: `${person.label}（${person.account}）`,
                                                  }))
                                                : roles.map((role) => ({
                                                      value: role.id,
                                                      label: role.name,
                                                  }))
                                        }
                                    />
                                )}
                            />
                        )}
                    />
                    <form.AppField
                        name="resource"
                        children={(field) => (
                            <field.SelectField
                                id="organization-scope-resource"
                                label="资源"
                                options={resources.map((item) => ({
                                    value: item.resource,
                                    label: resourceLabel(item.resource),
                                }))}
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.values.resource}
                        children={(resource) => {
                            const actions =
                                resources.find(
                                    (item) => item.resource === resource,
                                )?.actions ?? []
                            return (
                                <form.AppField
                                    name="actions"
                                    children={(field) => (
                                        <Field>
                                            <FieldLabel htmlFor="organization-scope-actions">
                                                动作
                                            </FieldLabel>
                                            <MultiOptionCombobox
                                                id="organization-scope-actions"
                                                aria-label="动作"
                                                value={field.state.value}
                                                options={actions.map(
                                                    (action) => ({
                                                        value: action,
                                                        label: actionLabel(
                                                            action,
                                                        ),
                                                    }),
                                                )}
                                                onValueChange={(ids) =>
                                                    field.handleChange(ids)
                                                }
                                                placeholder="选择动作"
                                            />
                                        </Field>
                                    )}
                                />
                            )
                        }}
                    />
                    <form.AppField
                        name="scopeType"
                        children={(field) => (
                            <field.SelectField
                                id="organization-scope-type"
                                label="范围类型"
                                options={Object.entries(SCOPE_TYPE_LABEL).map(
                                    ([value, label]) => ({ value, label }),
                                )}
                            />
                        )}
                    />
                    <form.AppField
                        name="targetDimension"
                        children={(field) => (
                            <field.SelectField
                                id="organization-scope-dimension"
                                label="目标维度"
                                options={Object.entries(DIMENSION_LABEL).map(
                                    ([value, label]) => ({
                                        value,
                                        label,
                                    }),
                                )}
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.values.scopeType}
                        children={(scopeType) =>
                            scopeType === "organization" ||
                            scopeType === "team" ? (
                                <>
                                    <form.AppField
                                        name="targetMode"
                                        children={(field) => (
                                            <field.SelectField
                                                id="organization-scope-mode"
                                                label="目标模式"
                                                options={Object.entries(
                                                    TARGET_MODE_LABEL,
                                                ).map(([value, label]) => ({
                                                    value,
                                                    label,
                                                }))}
                                            />
                                        )}
                                    />
                                    <form.Subscribe
                                        selector={(state) =>
                                            state.values.targetMode
                                        }
                                        children={(mode) =>
                                            mode === "explicit" ? (
                                                <form.AppField
                                                    name="scopeTargets"
                                                    children={(field) => (
                                                        <Field>
                                                            <FieldLabel htmlFor="organization-scope-targets">
                                                                组织目标
                                                            </FieldLabel>
                                                            <form.Subscribe
                                                                selector={(
                                                                    state,
                                                                ) =>
                                                                    state.values
                                                                        .targetDimension
                                                                }
                                                                children={(
                                                                    dimension,
                                                                ) => (
                                                                    <ScopeTargetPicker
                                                                        dimension={
                                                                            dimension
                                                                        }
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value
                                                                        }
                                                                        units={
                                                                            units
                                                                        }
                                                                        onChange={
                                                                            field.handleChange
                                                                        }
                                                                    />
                                                                )}
                                                            />
                                                        </Field>
                                                    )}
                                                />
                                            ) : null
                                        }
                                    />
                                    <form.Subscribe
                                        selector={(state) => ({
                                            mode: state.values.targetMode,
                                            dimension:
                                                state.values.targetDimension,
                                        })}
                                        children={({ mode, dimension }) =>
                                            dimension === "internal_org" &&
                                            (mode === "explicit" ||
                                                mode === "own_org") ? (
                                                <form.AppField
                                                    name="includeDescendants"
                                                    children={(field) => (
                                                        <field.SelectField
                                                            id="organization-scope-descendants"
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
                                            ) : null
                                        }
                                    />
                                </>
                            ) : null
                        }
                    />
                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>无法保存</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}
                    <DialogFooter className="flex min-w-0 flex-wrap justify-end gap-2">
                        <Button
                            id="organization-scope-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="organization-scope-submit"
                            type="submit"
                            disabled={submitting}
                        >
                            保存范围
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
