"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { CategoryCombobox } from "@/components/business/entity-comboboxes"
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
import {
    Field,
    FieldDescription,
    FieldGroup,
    FieldLabel,
} from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { toast } from "@/components/ui/toast"
import { useAdminsQuery } from "@/features/admin/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
    type ProductKind,
} from "@/features/master-data/types"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useResponsibilityRulesColumns } from "@/features/procurement-responsibilities/hooks/use-responsibility-rules-columns"
import {
    useProcurementResponsibilityRulesQuery,
    useSaveProcurementResponsibilityRuleMutation,
} from "@/features/procurement-responsibilities/queries"
import {
    PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL,
    type ProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/types"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

const RULE_TYPE_VALUES = [
    "SKU",
    "CATEGORY_SERVICE_REGION",
    "CATEGORY",
    "PRODUCT_KIND",
    "DEFAULT_DISPATCHER",
] as const

const ruleFormSchema = z
    .object({
        ruleType: z.enum(RULE_TYPE_VALUES),
        skuId: z.string(),
        categoryId: z.string(),
        serviceRegion: z.string(),
        productKind: z.string(),
        ownerUserId: z.string().min(1, "请选择采购负责人"),
        enabled: z.boolean(),
    })
    .superRefine((value, context) => {
        if (value.ruleType === "SKU" && !value.skuId) {
            context.addIssue({
                code: "custom",
                path: ["skuId"],
                message: "请选择 SKU",
            })
        }
        if (
            (value.ruleType === "CATEGORY" ||
                value.ruleType === "CATEGORY_SERVICE_REGION") &&
            !value.categoryId
        ) {
            context.addIssue({
                code: "custom",
                path: ["categoryId"],
                message: "请选择商品分类",
            })
        }
        if (
            value.ruleType === "CATEGORY_SERVICE_REGION" &&
            !value.serviceRegion.trim()
        ) {
            context.addIssue({
                code: "custom",
                path: ["serviceRegion"],
                message: "请输入服务区域",
            })
        }
        if (
            value.ruleType === "PRODUCT_KIND" &&
            !PRODUCT_KIND_VALUES.includes(value.productKind as ProductKind)
        ) {
            context.addIssue({
                code: "custom",
                path: ["productKind"],
                message: "请选择商品类型",
            })
        }
    })

type RuleFormValue = z.input<typeof ruleFormSchema>

function defaultValues(rule?: ProcurementResponsibilityRule): RuleFormValue {
    return {
        ruleType: rule?.ruleType ?? "SKU",
        skuId: rule?.skuId ?? "",
        categoryId: rule?.categoryId ?? "",
        serviceRegion: rule?.serviceRegion ?? "",
        productKind: rule?.productKind ?? "",
        ownerUserId: rule?.ownerUserId ?? "",
        enabled: rule?.enabled ?? true,
    }
}

function RuleDialog({
    open,
    onOpenChange,
    target,
    categories,
    ownerOptions,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target?: ProcurementResponsibilityRule
    categories: readonly {
        categoryId: string
        categoryCode: string
        categoryName: string
        parentId?: string
    }[]
    ownerOptions: readonly { value: string; label: string }[]
}) {
    const mutation = useSaveProcurementResponsibilityRuleMutation()
    const form = useAppForm({
        defaultValues: defaultValues(target),
        validators: { onChange: ruleFormSchema },
        onSubmit: async ({ value }) => {
            try {
                await mutation.mutateAsync({
                    ruleId: target?.ruleId,
                    ruleType: value.ruleType,
                    skuId: value.skuId || undefined,
                    categoryId: value.categoryId || undefined,
                    serviceRegion: value.serviceRegion || undefined,
                    productKind: value.productKind
                        ? (value.productKind as ProductKind)
                        : undefined,
                    ownerUserId: value.ownerUserId,
                    enabled: value.enabled,
                    expectedVersion: target?.version,
                })
                toast.add({
                    title: target ? "采购责任规则已更新" : "采购责任规则已新增",
                    description: "后续销售责任预览将按最新启用规则解析。",
                    type: "success",
                    timeout: 4000,
                })
                onOpenChange(false)
            } catch (error) {
                // MutationCache 已统一提示；此处吞回异常，避免表单重复报告。
                void error
            }
        },
    })

    React.useEffect(() => {
        if (open) form.reset(defaultValues(target))
    }, [form, open, target])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-2xl"
                closeButtonId="procurement-responsibility-rules-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>
                        {target ? "编辑采购责任规则" : "新增采购责任规则"}
                    </DialogTitle>
                    <DialogDescription>
                        按从具体到通用的层级维护负责人。销售人员只能查看解析结果，不能在销售单上改负责人。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <FieldGroup>
                        <form.AppField name="ruleType">
                            {(field) => (
                                <field.SelectField
                                    id="procurement-responsibility-rules-dialog-rule-type"
                                    label="规则类型"
                                    required
                                    allowClear={false}
                                    options={RULE_TYPE_VALUES.map((value) => ({
                                        value,
                                        label: PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[
                                            value
                                        ],
                                    }))}
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => state.values.ruleType}
                        >
                            {(ruleType) => (
                                <>
                                    {ruleType === "SKU" ? (
                                        <form.AppField name="skuId">
                                            {(field) => (
                                                <Field>
                                                    <FieldLabel>
                                                        公司 SKU
                                                        <span className="text-destructive">
                                                            *
                                                        </span>
                                                    </FieldLabel>
                                                    <CompanySkuSearchCombobox
                                                        id="procurement-responsibility-rules-dialog-sku"
                                                        purpose="form"
                                                        value={
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            field.handleChange(
                                                                value ?? "",
                                                            )
                                                        }
                                                        label="公司 SKU"
                                                        placeholder="搜索 SKU 或商品名称"
                                                    />
                                                </Field>
                                            )}
                                        </form.AppField>
                                    ) : null}
                                    {ruleType === "CATEGORY" ||
                                    ruleType === "CATEGORY_SERVICE_REGION" ? (
                                        <form.AppField name="categoryId">
                                            {(field) => (
                                                <Field>
                                                    <FieldLabel>
                                                        商品分类
                                                        <span className="text-destructive">
                                                            *
                                                        </span>
                                                    </FieldLabel>
                                                    <CategoryCombobox
                                                        id="procurement-responsibility-rules-dialog-category"
                                                        categories={categories}
                                                        value={
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            field.handleChange(
                                                                value ?? "",
                                                            )
                                                        }
                                                    />
                                                </Field>
                                            )}
                                        </form.AppField>
                                    ) : null}
                                    {ruleType === "CATEGORY_SERVICE_REGION" ? (
                                        <form.AppField name="serviceRegion">
                                            {(field) => (
                                                <field.TextField
                                                    id="procurement-responsibility-rules-dialog-service-region"
                                                    label="服务区域"
                                                    required
                                                    placeholder="例如：华东、上海市"
                                                />
                                            )}
                                        </form.AppField>
                                    ) : null}
                                    {ruleType === "PRODUCT_KIND" ? (
                                        <form.AppField name="productKind">
                                            {(field) => (
                                                <field.SelectField
                                                    id="procurement-responsibility-rules-dialog-product-kind"
                                                    label="商品类型"
                                                    required
                                                    allowClear={false}
                                                    options={PRODUCT_KIND_VALUES.map(
                                                        (value) => ({
                                                            value,
                                                            label: PRODUCT_KIND_LABELS[
                                                                value
                                                            ],
                                                        }),
                                                    )}
                                                />
                                            )}
                                        </form.AppField>
                                    ) : null}
                                </>
                            )}
                        </form.Subscribe>
                        <form.AppField name="ownerUserId">
                            {(field) => (
                                <field.SelectField
                                    id="procurement-responsibility-rules-dialog-owner"
                                    label="采购负责人"
                                    required
                                    options={ownerOptions}
                                    placeholder="选择现有账号"
                                    allowClear={false}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="enabled">
                            {(field) => (
                                <Field orientation="horizontal">
                                    <div className="flex-1">
                                        <FieldLabel htmlFor="procurement-responsibility-rules-dialog-enabled">
                                            启用规则
                                        </FieldLabel>
                                        <FieldDescription>
                                            停用后不再参与销售行负责人解析。
                                        </FieldDescription>
                                    </div>
                                    <Switch
                                        id="procurement-responsibility-rules-dialog-enabled"
                                        checked={field.state.value}
                                        onCheckedChange={(checked) =>
                                            field.handleChange(Boolean(checked))
                                        }
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    </FieldGroup>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    id="procurement-responsibility-rules-dialog-cancel"
                                    type="button"
                                    variant="outline"
                                />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.Subscribe selector={(state) => state.canSubmit}>
                            {(canSubmit) => (
                                <Button
                                    id="procurement-responsibility-rules-dialog-save"
                                    type="submit"
                                    data-testid="procurement-responsibility-save"
                                    disabled={!canSubmit || mutation.isPending}
                                >
                                    {mutation.isPending
                                        ? "保存中…"
                                        : "保存规则"}
                                </Button>
                            )}
                        </form.Subscribe>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

export function ProcurementResponsibilityRulesPage() {
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canList = hasPermission(
        permissions,
        "procurement_responsibility:list",
    )
    const canManage = hasPermission(
        permissions,
        "procurement_responsibility:manage",
    )
    const rulesQuery = useProcurementResponsibilityRulesQuery()
    const adminsQuery = useAdminsQuery()
    const categoriesQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "all",
        revisionTiming: "current",
    })
    const dependenciesPending =
        Boolean(adminsQuery.isPending) || Boolean(categoriesQuery.isPending)
    const dependenciesFailed =
        Boolean(adminsQuery.isError) || Boolean(categoriesQuery.isError)
    const dependencyError = adminsQuery.error ?? categoriesQuery.error
    const [dialogOpen, setDialogOpen] = React.useState(false)
    const [editing, setEditing] =
        React.useState<ProcurementResponsibilityRule>()
    const openCreate = React.useCallback(() => {
        setEditing(undefined)
        setDialogOpen(true)
    }, [])
    const openEdit = React.useCallback(
        (rule: ProcurementResponsibilityRule) => {
            setEditing(rule)
            setDialogOpen(true)
        },
        [],
    )
    const columns = useResponsibilityRulesColumns()

    if (profileQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="采购责任规则" density="compact" />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (profileQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="采购责任规则" density="compact" />
                <BusinessFailureState
                    kind="system"
                    title="权限信息加载失败"
                    description={getErrorMessage(
                        profileQuery.error,
                        "暂时无法核对采购责任规则权限。",
                    )}
                    action={
                        <Button
                            id="procurement-responsibility-rules-profile-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void profileQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!canList && !canManage) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="采购责任规则" density="compact" />
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看采购责任规则。"
                />
            </PageScaffold>
        )
    }

    const categories = (categoriesQuery.data?.rows ?? []).map((row) => ({
        categoryId: row.stableId,
        categoryCode: row.dictionaryCode ?? row.stableNo,
        categoryName: row.name,
        parentId: row.parentStableId,
    }))
    const ownerOptions = (adminsQuery.data ?? []).map((account) => ({
        value: account.id,
        label: `${account.name} · ${account.account}`,
    }))
    const rows = [...(rulesQuery.data ?? [])]

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="采购责任规则"
                description="维护销售实物行到采购负责人的分配规则；越具体的规则优先命中。"
                actions={
                    canManage ? (
                        <Button
                            id="procurement-responsibility-rules-create"
                            type="button"
                            size="sm"
                            data-testid="procurement-responsibility-create"
                            disabled={dependenciesPending || dependenciesFailed}
                            title={
                                dependenciesPending
                                    ? "正在加载负责人和分类选项"
                                    : dependenciesFailed
                                      ? "负责人或分类选项加载失败，请先重试"
                                      : undefined
                            }
                            onClick={openCreate}
                        >
                            <PlusIcon data-icon="inline-start" />
                            新增规则
                        </Button>
                    ) : undefined
                }
            />
            {canManage && dependenciesFailed ? (
                <BusinessFailureState
                    kind="system"
                    title="规则编辑依赖加载失败"
                    description={getErrorMessage(
                        dependencyError,
                        "暂时无法读取采购负责人或商品分类，当前不能新增或编辑规则。",
                    )}
                    action={
                        <Button
                            id="procurement-responsibility-rules-retry-dependencies"
                            type="button"
                            variant="outline"
                            onClick={() => {
                                void adminsQuery.refetch()
                                void categoriesQuery.refetch()
                            }}
                        >
                            重试依赖数据
                        </Button>
                    }
                />
            ) : canManage && dependenciesPending ? (
                <Alert variant="info">
                    <AlertTitle>选项加载中</AlertTitle>
                    <AlertDescription>
                        正在加载采购负责人和商品分类，完成后可编辑规则。
                    </AlertDescription>
                </Alert>
            ) : null}
            <BusinessTableFrame
                data-testid="procurement-responsibility-rules"
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        责任规则列表
                        <span
                            className="font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {rows.length} 条
                        </span>
                    </span>
                }
                description="支持 SKU、分类与区域、分类、商品类型和默认调度人五个层级。"
                table={
                    <DataTable
                        id="procurement-responsibility-rules-table"
                        data={rows}
                        columns={columns}
                        getRowId={(row) => row.ruleId}
                        rowCount={rows.length}
                        layout="flush"
                        loading={
                            rulesQuery.isPending ||
                            Boolean(rulesQuery.isFetching)
                        }
                        showPagination={false}
                        errorState={
                            rulesQuery.isError ? (
                                <BusinessFailureState
                                    kind="system"
                                    title="规则加载失败"
                                    description={getErrorMessage(
                                        rulesQuery.error,
                                        "暂时无法读取采购责任规则。",
                                    )}
                                    action={
                                        <Button
                                            id="procurement-responsibility-rules-retry"
                                            type="button"
                                            variant="outline"
                                            onClick={() =>
                                                void rulesQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !rulesQuery.isError && rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind="no-data"
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title="还没有采购责任规则"
                                    description="请先新增默认调度人，再逐步补充更具体的规则。"
                                    action={
                                        canManage &&
                                        !dependenciesPending &&
                                        !dependenciesFailed ? (
                                            <Button
                                                id="procurement-responsibility-rules-empty-create"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={openCreate}
                                            >
                                                新增规则
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowOpen={canManage ? openEdit : undefined}
                    />
                }
            />
            {canManage ? (
                <RuleDialog
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    target={editing}
                    categories={categories}
                    ownerOptions={ownerOptions}
                />
            ) : null}
        </PageScaffold>
    )
}
