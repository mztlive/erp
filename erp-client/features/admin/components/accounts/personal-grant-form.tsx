"use client"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm, toFieldErrors } from "@/components/form"
import { FieldError } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import {
    AlertDialog,
    AlertDialogContent,
    AlertDialogHeader,
    AlertDialogTitle,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogCancel,
    AlertDialogAction,
} from "@/components/ui/alert-dialog"
import { Checkbox } from "@/components/ui/checkbox"
import { OptionCombobox } from "@/components/business/option-combobox"
import { ScopeTargetPicker } from "@/features/organization/components/scope-target-picker"
import type { OrgUnit } from "@/features/organization/types"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { useSavePersonScope } from "../../hooks/use-person-data-scopes"
import {
    personScopeDescription,
    personScopeDefaults,
    type PersonScopeInput,
    type PersonScopeList,
    type PersonScopeTerm,
} from "../../api/person-data-scopes"

export function PersonalGrantForm({
    userId,
    resource,
    onSaved,
    name,
    data,
    units,
    draft,
    onDraftChange,
    onDone,
    onReload,
}: {
    userId: string
    resource: string
    onSaved: () => void
    name: string
    data: PersonScopeList
    units: OrgUnit[]
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
    onDone: () => void
    onReload: () => void
}) {
    const save = useSavePersonScope(userId)
    const [discarding, setDiscarding] = React.useState(false)
    const [initialDraft] = React.useState(draft)
    const [version] = React.useState(
        draft?.policyVersion ?? data.policy_version,
    )
    const [error, setError] = React.useState<string | null>(null)
    const [defaults] = React.useState(() => personScopeDefaults(data, resource))
    const stale = version !== data.policy_version
    const schema = z
        .object({
            confirmed: z.boolean().refine((v) => v, "请明确选择新的范围"),
            resource: z.string().min(1, "请选择业务"),
            dimension: z.enum([
                "internal_org",
                "warehouse",
                "settlement_party",
            ]),
            actions: z.array(z.string()).min(1, "请选择操作"),
            mode: z.enum(["self", "own_org", "explicit", "company"]),
            org_ids: z.array(z.string()),
            warehouse_ids: z.array(z.string()),
            settlement_ids: z.array(z.string()),
            include_descendants: z.boolean(),
        })
        .superRefine((value, ctx) => {
            const chosen = data.businesses.find(
                (b) => b.resource === value.resource,
            )
            const business = chosen
                ? {
                      ...chosen,
                      dimensions:
                          chosen.resource === "approval_instance"
                              ? [value.dimension]
                              : chosen.dimensions,
                  }
                : undefined
            if (!business) return
            if (value.mode === "company") return
            for (const dimension of business.resource === "approval_instance"
                ? [value.dimension]
                : business.dimensions) {
                const field =
                    dimension === "internal_org"
                        ? "org_ids"
                        : dimension === "warehouse"
                          ? "warehouse_ids"
                          : "settlement_ids"
                if (
                    (dimension !== "internal_org" ||
                        value.mode === "explicit") &&
                    !value[field].length
                )
                    ctx.addIssue({
                        code: "custom",
                        path: [field],
                        message: "请选择范围目标",
                    })
            }
        })
    const form = useAppForm({
        defaultValues: initialDraft?.values ?? defaults,
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            if (stale) {
                setError("权限配置已变化，草稿已保留。请刷新后重新核对。")
                return
            }
            const chosen = data.businesses.find(
                (b) => b.resource === value.resource,
            )
            const business = chosen
                ? {
                      ...chosen,
                      dimensions:
                          chosen.resource === "approval_instance"
                              ? [value.dimension]
                              : chosen.dimensions,
                  }
                : undefined
            if (!business) return
            const terms: PersonScopeTerm[] =
                value.mode === "company"
                    ? [
                          {
                              scope_type: "company",
                              target_dimension: business.dimensions[0],
                              target_mode: null,
                              include_descendants: null,
                              scope_targets: [],
                          },
                      ]
                    : (business.resource === "approval_instance"
                          ? [value.dimension]
                          : business.dimensions
                      ).map((d) => ({
                          scope_type:
                              d === "internal_org" && value.mode === "self"
                                  ? "self_owned"
                                  : "organization",
                          target_dimension: d,
                          target_mode:
                              d === "internal_org" && value.mode === "self"
                                  ? null
                                  : d === "internal_org" &&
                                      value.mode === "own_org"
                                    ? "own_org"
                                    : "explicit",
                          include_descendants:
                              d === "internal_org" && value.mode !== "self"
                                  ? value.include_descendants
                                  : null,
                          scope_targets:
                              d === "internal_org"
                                  ? value.mode === "explicit"
                                      ? value.org_ids
                                      : []
                                  : d === "warehouse"
                                    ? value.warehouse_ids
                                    : value.settlement_ids,
                      }))
            setError(null)
            try {
                await save.mutateAsync({
                    resource: value.resource,
                    actions: value.actions,
                    terms,
                    version,
                })
                onSaved()
                onDone()
            } catch (e) {
                setError(getErrorMessage(e, "保存失败，请刷新后重试"))
            }
        },
    })
    const value = useStore(form.store, (s) => s.values)
    const chosen = data.businesses.find((b) => b.resource === value.resource)
    const business = chosen
        ? {
              ...chosen,
              dimensions:
                  chosen.resource === "approval_instance"
                      ? [value.dimension]
                      : chosen.dimensions,
          }
        : undefined
    React.useEffect(() => {
        onDraftChange({ values: value, policyVersion: version })
    }, [value, version, onDraftChange])
    const leave = () => {
        if (save.isPending) return
        if (
            initialDraft ||
            JSON.stringify(value) !== JSON.stringify(defaults)
        ) {
            setDiscarding(true)
        } else onDone()
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) leave()
            }}
        >
            <DialogContent
                id="person-scope-dialog"
                closeButtonId="person-scope-dialog-close"
                showCloseButton={!save.isPending}
                className="max-h-[85dvh] overflow-y-auto sm:max-w-xl"
                finalFocus={() =>
                    document.getElementById(
                        `person-scope-${toAutomationIdSegment(resource)}-edit`,
                    )
                }
            >
                <DialogHeader>
                    <DialogTitle>
                        {resourceLabel(resource)} · 数据范围
                    </DialogTitle>
                    <DialogDescription>
                        为{name}设置已有操作可处理的数据。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4 text-sm"
                    onSubmit={(e) => {
                        e.preventDefault()
                        e.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset disabled={save.isPending} className="space-y-4">
                        {stale && (
                            <div
                                role="status"
                                className="text-xs text-amber-700"
                            >
                                配置已变化，当前草稿不能直接保存。
                                <Button
                                    id="person-scope-reload"
                                    type="button"
                                    variant="link"
                                    onClick={() => {
                                        if (
                                            window.confirm(
                                                "放弃当前草稿并刷新配置？",
                                            )
                                        ) {
                                            onDone()
                                            onReload()
                                        }
                                    }}
                                >
                                    刷新配置
                                </Button>
                            </div>
                        )}
                        {business && (
                            <>
                                {!value.confirmed && (
                                    <p
                                        role="status"
                                        className="text-xs text-amber-700"
                                    >
                                        当前各操作范围不同或包含迁移条件。请选择本次的新范围，保存会统一所选操作。
                                    </p>
                                )}
                                <form.Field name="confirmed">
                                    {(field) => (
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    )}
                                </form.Field>
                                {business.resource === "approval_instance" && (
                                    <form.Field name="dimension">
                                        {(field) => (
                                            <div>
                                                <label htmlFor="person-scope-dimension">
                                                    审批范围维度
                                                </label>
                                                <OptionCombobox
                                                    id="person-scope-dimension"
                                                    aria-label="审批范围维度"
                                                    className="mt-2"
                                                    value={field.state.value}
                                                    allowClear={false}
                                                    disabled={save.isPending}
                                                    options={[
                                                        {
                                                            value: "internal_org",
                                                            label: "业务部门/负责人",
                                                        },
                                                        {
                                                            value: "warehouse",
                                                            label: "仓库",
                                                        },
                                                        {
                                                            value: "settlement_party",
                                                            label: "结算主体",
                                                        },
                                                    ]}
                                                    onBlur={field.handleBlur}
                                                    onValueChange={(
                                                        dimension,
                                                    ) => {
                                                        if (!dimension) return
                                                        const d =
                                                            dimension as PersonScopeInput["dimension"]
                                                        field.handleChange(d)
                                                        form.setFieldValue(
                                                            "mode",
                                                            d === "internal_org"
                                                                ? "self"
                                                                : "explicit",
                                                        )
                                                    }}
                                                />
                                            </div>
                                        )}
                                    </form.Field>
                                )}
                                <details className="space-y-3">
                                    <summary
                                        id="person-scope-actions-expand"
                                        className="cursor-pointer"
                                    >
                                        适用操作：
                                        {value.actions.length ===
                                        business.actions.length
                                            ? "此业务全部已有操作"
                                            : value.actions
                                                  .map(actionLabel)
                                                  .join("、") || "未选择"}{" "}
                                        · 按操作区分
                                    </summary>
                                    <form.Field name="actions">
                                        {(field) => (
                                            <div className="flex flex-wrap gap-4">
                                                {business.actions.map(
                                                    (action) => (
                                                        <label
                                                            key={action}
                                                            className="flex items-center gap-2"
                                                        >
                                                            <Checkbox
                                                                id={`person-scope-action-${toAutomationIdSegment(action)}`}
                                                                checked={field.state.value.includes(
                                                                    action,
                                                                )}
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) =>
                                                                    field.handleChange(
                                                                        checked
                                                                            ? [
                                                                                  ...field
                                                                                      .state
                                                                                      .value,
                                                                                  action,
                                                                              ]
                                                                            : field.state.value.filter(
                                                                                  (
                                                                                      a,
                                                                                  ) =>
                                                                                      a !==
                                                                                      action,
                                                                              ),
                                                                    )
                                                                }
                                                            />
                                                            {actionLabel(
                                                                action,
                                                            )}
                                                        </label>
                                                    ),
                                                )}
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            </div>
                                        )}
                                    </form.Field>
                                </details>
                                <div className="space-y-1 text-xs text-muted-foreground">
                                    {value.actions.map((action) => (
                                        <p key={action}>
                                            当前{actionLabel(action)}：
                                            {personScopeDescription(
                                                data.items.find(
                                                    (s) =>
                                                        s.resource ===
                                                            value.resource &&
                                                        s.action === action,
                                                ),
                                                new Map(
                                                    units.map((u) => [
                                                        u.id,
                                                        u.name,
                                                    ]),
                                                ),
                                            )}
                                        </p>
                                    ))}
                                </div>
                                <form.Field name="mode">
                                    {(field) => (
                                        <fieldset className="space-y-2">
                                            <legend className="mb-2 font-medium">
                                                可以处理哪些数据？
                                            </legend>
                                            {(
                                                [
                                                    ["self", "本人负责"],
                                                    ["own_org", "自己所属部门"],
                                                    [
                                                        "explicit",
                                                        business.dimensions.includes(
                                                            "internal_org",
                                                        )
                                                            ? "指定部门"
                                                            : "指定对象",
                                                    ],
                                                    ["company", "公司范围"],
                                                ] as const
                                            )
                                                .filter(
                                                    ([mode]) =>
                                                        business.dimensions.includes(
                                                            "internal_org",
                                                        ) ||
                                                        ![
                                                            "self",
                                                            "own_org",
                                                        ].includes(mode),
                                                )
                                                .map(([mode, label]) => (
                                                    <label
                                                        htmlFor={`person-scope-mode-${mode}`}
                                                        key={mode}
                                                        className="flex items-center gap-2"
                                                    >
                                                        <input
                                                            id={`person-scope-mode-${mode}`}
                                                            type="radio"
                                                            name="person-scope-mode"
                                                            checked={
                                                                value.confirmed &&
                                                                field.state
                                                                    .value ===
                                                                    mode
                                                            }
                                                            onChange={() => {
                                                                field.handleChange(
                                                                    mode,
                                                                )
                                                                form.setFieldValue(
                                                                    "confirmed",
                                                                    true,
                                                                )
                                                            }}
                                                        />
                                                        {label}
                                                    </label>
                                                ))}
                                        </fieldset>
                                    )}
                                </form.Field>
                                {value.mode !== "company" &&
                                    business.dimensions.map((dimension) => {
                                        if (
                                            dimension === "internal_org" &&
                                            value.mode !== "explicit"
                                        )
                                            return null
                                        const key =
                                            dimension === "internal_org"
                                                ? "org_ids"
                                                : dimension === "warehouse"
                                                  ? "warehouse_ids"
                                                  : "settlement_ids"
                                        return (
                                            <form.Field
                                                key={dimension}
                                                name={key}
                                            >
                                                {(field) => (
                                                    <div className="space-y-2">
                                                        <p>
                                                            {dimension ===
                                                            "internal_org"
                                                                ? "部门"
                                                                : dimension ===
                                                                    "warehouse"
                                                                  ? "仓库"
                                                                  : "结算主体"}
                                                        </p>
                                                        <ScopeTargetPicker
                                                            id={`person-scope-target-${dimension}`}
                                                            dimension={
                                                                dimension
                                                            }
                                                            value={
                                                                field.state
                                                                    .value
                                                            }
                                                            onChange={
                                                                field.handleChange
                                                            }
                                                            units={units}
                                                        />
                                                        <FieldError
                                                            errors={toFieldErrors(
                                                                field.state.meta
                                                                    .errors,
                                                            )}
                                                        />
                                                    </div>
                                                )}
                                            </form.Field>
                                        )
                                    })}
                                {business.dimensions.includes("internal_org") &&
                                    ["explicit", "own_org"].includes(
                                        value.mode,
                                    ) && (
                                        <form.Field name="include_descendants">
                                            {(field) => (
                                                <label
                                                    htmlFor="person-scope-descendants"
                                                    className="flex items-center gap-2"
                                                >
                                                    <Checkbox
                                                        id="person-scope-descendants"
                                                        checked={
                                                            field.state.value
                                                        }
                                                        onCheckedChange={(v) =>
                                                            field.handleChange(
                                                                v === true,
                                                            )
                                                        }
                                                    />
                                                    包含下级部门
                                                </label>
                                            )}
                                        </form.Field>
                                    )}
                                <p className="text-xs text-muted-foreground">
                                    保存后替换此人所选操作的范围。其他操作、其他人员保持原配置；业务状态及审批资格仍需满足。
                                </p>
                            </>
                        )}
                        {error && (
                            <p
                                role="alert"
                                className="text-xs text-destructive"
                            >
                                {error}
                            </p>
                        )}
                    </fieldset>
                    <div className="sticky bottom-0 flex justify-end gap-2 border-t bg-popover pt-4">
                        <Button
                            id="person-scope-cancel"
                            size="sm"
                            type="button"
                            variant="outline"
                            disabled={save.isPending}
                            onClick={leave}
                        >
                            取消
                        </Button>
                        <Button
                            id="person-scope-save"
                            size="sm"
                            type="submit"
                            disabled={save.isPending || stale}
                        >
                            {save.isPending ? "保存中…" : "保存数据范围"}
                        </Button>
                    </div>
                </form>
                <AlertDialog open={discarding} onOpenChange={setDiscarding}>
                    <AlertDialogContent>
                        <AlertDialogHeader>
                            <AlertDialogTitle>放弃本次修改？</AlertDialogTitle>
                            <AlertDialogDescription>
                                数据范围尚未保存，放弃后将保留原有配置。
                            </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                            <AlertDialogCancel id="person-scope-discard-cancel">
                                继续编辑
                            </AlertDialogCancel>
                            <AlertDialogAction
                                id="person-scope-discard-confirm"
                                onClick={onDone}
                            >
                                放弃修改
                            </AlertDialogAction>
                        </AlertDialogFooter>
                    </AlertDialogContent>
                </AlertDialog>
            </DialogContent>
        </Dialog>
    )
}
