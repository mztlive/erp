"use client"

import * as React from "react"
import Link from "next/link"
import { LayersIcon } from "lucide-react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PERMISSION_BY_CODE } from "@/lib/permission-catalog"
import {
    useGenerateBuiltinRoles,
    useRoleTemplates,
} from "../../hooks/use-role-templates"
import type {
    BuiltinRoleCatalog,
    BuiltinRoleState,
    BuiltinRoleTemplate,
    GeneratedBuiltinRole,
} from "../../api/role-templates"

const STATE_LABEL: Record<BuiltinRoleState, string> = {
    missing: "待生成",
    existing: "已存在，保留配置",
    disabled: "已停用，保留状态",
    deleted: "已删除，不自动恢复",
}

/** 部署人员从后端模板预览并生成普通岗位角色。 */
export function BuiltinRoleDialog() {
    const [open, setOpen] = React.useState(false)
    const [results, setResults] = React.useState<GeneratedBuiltinRole[] | null>(
        null,
    )
    const catalog = useRoleTemplates(open)
    const generate = useGenerateBuiltinRoles()
    return (
        <Dialog
            open={open}
            onOpenChange={(value) => {
                if (generate.isPending) return
                setOpen(value)
                setResults(null)
            }}
        >
            <DialogTrigger
                id="roles-builtin-open"
                render={<Button variant="outline" size="sm" />}
            >
                <LayersIcon className="size-3.5" aria-hidden="true" />
                从内建岗位生成
            </DialogTrigger>
            <DialogContent
                className="flex max-h-[85vh] flex-col sm:max-w-3xl"
                closeButtonId="roles-builtin-close"
                showCloseButton={!generate.isPending}
            >
                <DialogHeader>
                    <DialogTitle>生成岗位角色</DialogTitle>
                    <DialogDescription>
                        选择适合本公司的岗位，查看职责和权限后生成。已有角色保留当前配置，生成后可以继续编辑。
                    </DialogDescription>
                </DialogHeader>
                {results ? (
                    <GenerationResult
                        results={results}
                        onClose={() => setOpen(false)}
                    />
                ) : catalog.isPending ? (
                    <p role="status">正在加载岗位模板…</p>
                ) : catalog.isError ? (
                    <div role="alert" className="space-y-3">
                        <p>
                            {getErrorMessage(catalog.error, "岗位模板加载失败")}
                        </p>
                        <Button
                            id="roles-builtin-retry"
                            variant="outline"
                            onClick={() => void catalog.refetch()}
                        >
                            重新加载
                        </Button>
                    </div>
                ) : catalog.data ? (
                    <TemplateSelection
                        catalog={catalog.data}
                        pending={generate.isPending}
                        onGenerate={async (ids, version) => {
                            setResults(
                                await generate.mutateAsync({
                                    template_ids: ids,
                                    expected_policy_version: version,
                                }),
                            )
                        }}
                        onReload={() => void catalog.refetch()}
                        onClose={() => setOpen(false)}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

function TemplateSelection({
    catalog,
    pending,
    onGenerate,
    onReload,
    onClose,
}: {
    catalog: BuiltinRoleCatalog
    pending: boolean
    onGenerate: (ids: string[], version: number) => Promise<void>
    onReload: () => void
    onClose: () => void
}) {
    const [error, setError] = React.useState<string | null>(null)
    const [previewVersion] = React.useState(catalog.policy_version)
    const stale = previewVersion !== catalog.policy_version
    const form = useAppForm({
        defaultValues: {
            ids: catalog.templates
                .filter((item) => item.recommended && item.can_generate)
                .map((item) => item.id),
        },
        validators: {
            onChange: z.object({
                ids: z.array(z.string()).min(1, "请选择至少一个岗位"),
            }),
        },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                await onGenerate(value.ids, previewVersion)
            } catch (cause) {
                setError(
                    getErrorMessage(
                        cause,
                        "生成失败，请重新加载后核对岗位状态。",
                    ),
                )
            }
        },
    })
    return (
        <form
            className="flex min-h-0 flex-1 flex-col gap-4"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <p className="text-sm text-muted-foreground">
                财务分岗时选择“财务总监、出纳、开票”；“财务”是包含全部财务能力的可选综合岗位。分岗人员不要同时绑定综合财务角色。角色生成后，在人员资料分配角色和数据范围，并指定审批、付款、开票及仓库经办人。
            </p>
            <div className="min-h-0 overflow-y-auto divide-y rounded-lg border">
                <form.AppField name="ids">
                    {(field) =>
                        catalog.templates.map((template) => (
                            <TemplateRow
                                key={template.id}
                                template={template}
                                checked={field.state.value.includes(
                                    template.id,
                                )}
                                disabled={
                                    pending || stale || !template.can_generate
                                }
                                onChange={(checked) =>
                                    field.handleChange(
                                        checked
                                            ? [
                                                  ...field.state.value,
                                                  template.id,
                                              ]
                                            : field.state.value.filter(
                                                  (id) => id !== template.id,
                                              ),
                                    )
                                }
                            />
                        ))
                    }
                </form.AppField>
            </div>
            {(error || stale) && (
                <p role="alert" className="text-sm text-destructive">
                    {stale
                        ? "权限配置已更新，请关闭后重新打开，核对岗位再生成。"
                        : error}
                </p>
            )}
            <DialogFooter>
                {error && (
                    <Button
                        id="roles-builtin-refresh"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={onReload}
                    >
                        重新加载状态
                    </Button>
                )}
                <Button
                    id="roles-builtin-cancel"
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={onClose}
                >
                    关闭
                </Button>
                <form.Subscribe selector={(state) => state.values.ids}>
                    {(ids) => (
                        <Button
                            id="roles-builtin-generate"
                            type="submit"
                            disabled={pending || stale || ids.length === 0}
                        >
                            {pending
                                ? "正在生成…"
                                : `生成所选 ${ids.length} 个岗位`}
                        </Button>
                    )}
                </form.Subscribe>
            </DialogFooter>
        </form>
    )
}

function TemplateRow({
    template,
    checked,
    disabled,
    onChange,
}: {
    template: BuiltinRoleTemplate
    checked: boolean
    disabled: boolean
    onChange: (checked: boolean) => void
}) {
    const [expanded, setExpanded] = React.useState(false)
    const id = `roles-builtin-${toAutomationIdSegment(template.id)}`
    const labels = template.permissions
        .map((code) => {
            if (code.endsWith(":*")) {
                const resource = code.slice(0, -2)
                return [...PERMISSION_BY_CODE.values()]
                    .filter((permission) => permission.resource === resource)
                    .map((permission) => permission.description)
            }
            return [PERMISSION_BY_CODE.get(code)?.description ?? "专项业务授权"]
        })
        .flat()
    return (
        <div className="space-y-2 p-3">
            <div className="flex items-start gap-3">
                <Checkbox
                    id={`${id}-select`}
                    checked={checked}
                    disabled={disabled}
                    onCheckedChange={(value) => onChange(value === true)}
                    aria-label={`选择${template.name}`}
                />
                <div className="min-w-0 flex-1">
                    <label htmlFor={`${id}-select`} className="font-medium">
                        {template.name}
                    </label>
                    <span className="ml-2 text-xs text-muted-foreground">
                        {STATE_LABEL[template.state]}
                    </span>
                    <p className="mt-1 text-sm text-muted-foreground">
                        {template.description}
                    </p>
                    {template.existing_name && (
                        <p className="text-xs text-muted-foreground">
                            当前角色：{template.existing_name}
                            。请在角色列表核对已有权限。
                        </p>
                    )}
                    {template.state === "missing" && !template.can_generate && (
                        <p className="text-xs text-destructive">
                            当前账号不能授予此岗位的全部权限，请由超级管理员生成。
                        </p>
                    )}
                </div>
                <Button
                    id={`${id}-details`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-expanded={expanded}
                    aria-controls={`${id}-content`}
                    onClick={() => setExpanded(!expanded)}
                >
                    {expanded ? "收起" : "查看权限"}
                </Button>
            </div>
            {expanded && (
                <div id={`${id}-content`} className="space-y-3 pl-7 text-sm">
                    <div>
                        <p className="font-medium">操作权限</p>
                        <p className="mt-1 text-muted-foreground">
                            {[...new Set(labels)].join("、")}
                        </p>
                    </div>
                    <div>
                        <p className="font-medium">人员上岗配置</p>
                        <ul className="mt-1 list-disc space-y-1 pl-5 text-muted-foreground">
                            {template.setup_requirements.map((requirement) => (
                                <li key={requirement}>{requirement}</li>
                            ))}
                        </ul>
                    </div>
                </div>
            )}
        </div>
    )
}

function GenerationResult({
    results,
    onClose,
}: {
    results: GeneratedBuiltinRole[]
    onClose: () => void
}) {
    return (
        <>
            <p role="status">
                已生成 {results.filter((result) => result.created).length}{" "}
                个岗位，保留{" "}
                {results.filter((result) => !result.created).length}{" "}
                个已有角色。
            </p>
            <ul className="min-h-0 space-y-2 overflow-y-auto">
                {results.map((result) => (
                    <li
                        key={result.id}
                        className="flex items-center justify-between gap-3 border-b pb-2"
                    >
                        <span>
                            {result.name} ·{" "}
                            {result.created
                                ? "已生成"
                                : STATE_LABEL[result.state]}
                        </span>
                        {result.state === "existing" && (
                            <Link
                                id={`roles-builtin-result-${toAutomationIdSegment(result.id)}`}
                                className="text-sm underline"
                                href={`/system/roles/${encodeURIComponent(result.id)}/edit`}
                            >
                                查看角色
                            </Link>
                        )}
                    </li>
                ))}
            </ul>
            <p className="text-sm text-muted-foreground">
                下一步：到“组织与人员”分配岗位角色，按岗位说明配置数据范围和业务责任。
            </p>
            <DialogFooter>
                <Button id="roles-builtin-result-close" onClick={onClose}>
                    完成
                </Button>
            </DialogFooter>
        </>
    )
}
