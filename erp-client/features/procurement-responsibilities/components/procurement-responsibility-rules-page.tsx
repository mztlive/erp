"use client"

import * as React from "react"
import {
    OwnerPoolField,
    AssignmentStrategyField,
} from "@/components/business/responsibility-pool/fields"
import {
    ownerPoolSchema,
    assignmentStrategySchema,
} from "@/components/business/responsibility-pool/model"
import { PlusIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { CategoryCombobox } from "@/components/business/entity-comboboxes"
import { useAppForm, applyApiFormError } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
    "INQUIRY",
    "SKU",
    "CATEGORY",
    "PRODUCT_KIND",
    "DEFAULT_DISPATCHER",
] as const

const ruleFormSchema = z
    .object({
        ruleType: z.enum(RULE_TYPE_VALUES),
        skuId: z.string(),
        categoryId: z.string(),
        productKind: z.string(),
        ownerUserIds: ownerPoolSchema,
        strategy: assignmentStrategySchema,
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
        if (value.ruleType === "CATEGORY" && !value.categoryId) {
            context.addIssue({
                code: "custom",
                path: ["categoryId"],
                message: "请选择商品分类",
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
        productKind: rule?.productKind ?? "",
        ownerUserIds: [...(rule?.ownerUserIds ?? [])],
        strategy: rule?.strategy ?? ("ROUND_ROBIN" as const),
        enabled: rule?.enabled ?? true,
    }
}

function RuleDialog({
    open,
    onRefreshCategories,
    onRefreshOwners,
    onOpenChange,
    target,
    categories,
    ownerOptions,
}: {
    open: boolean
    onRefreshCategories: () => Promise<unknown>
    onRefreshOwners: () => Promise<unknown>
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
                    productKind: value.productKind
                        ? (value.productKind as ProductKind)
                        : undefined,
                    ownerUserIds: value.ownerUserIds,
                    strategy: value.strategy,
                    enabled: value.enabled,
                    expectedVersion: target?.version,
                })
                toast.add({
                    title: target ? "采购责任规则已更新" : "采购责任规则已新增",
                    description:
                        "后续采购任务按最新启用规则分派，已有任务负责人保持不变。",
                    type: "success",
                    timeout: 4000,
                })
                onOpenChange(false)
            } catch (error) {
                applyApiFormError(form, error)
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
                        每条规则配置一个负责人池。询价规则整单优先，其余按
                        SKU、分类、商品类型、默认池依次匹配。
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
                                    {ruleType === "CATEGORY" ? (
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
                                                        onRefresh={
                                                            onRefreshCategories
                                                        }
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
                        <form.AppField name="ownerUserIds">
                            {() => (
                                <OwnerPoolField
                                    id="procurement-responsibility-rules-dialog-owner"
                                    options={ownerOptions}
                                    selectedNames={Object.fromEntries(
                                        (target?.ownerUserIds ?? []).map(
                                            (id, index) => [
                                                id,
                                                target?.ownerNames[index] ??
                                                    "已选负责人",
                                            ],
                                        ),
                                    )}
                                    onRefresh={onRefreshOwners}
                                    description="按选择顺序轮流分派；同一单命中本规则的商品交给同一人。需要调整顺序时，移除后重新选择。"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="strategy">
                            {() => (
                                <AssignmentStrategyField id="procurement-responsibility-rules-dialog-strategy" />
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
                        <form.Subscribe
                            selector={(state) =>
                                [state.canSubmit, state.isSubmitting] as const
                            }
                        >
                            {([canSubmit, isSubmitting]) => {
                                const pending =
                                    isSubmitting || mutation.isPending
                                return (
                                    <LoadingButton
                                        loading={pending}
                                        id="procurement-responsibility-rules-dialog-save"
                                        type="submit"
                                        data-testid="procurement-responsibility-save"
                                        disabled={!canSubmit || pending}
                                    >
                                        {pending ? "保存中…" : "保存规则"}
                                    </LoadingButton>
                                )
                            }}
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
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader eyebrow="基础资料" title="采购责任规则" />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (profileQuery.isError) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader eyebrow="基础资料" title="采购责任规则" />
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
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader eyebrow="基础资料" title="采购责任规则" />
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
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                className="pb-4 md:pb-4"
                eyebrow="基础资料"
                title="采购责任规则"
                description="每条规则维护一个负责人池，各池独立轮询。询价规则启用后，询价整单优先进入该池。"
            >
                {canManage ? (
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
                ) : null}
            </ListWorkspaceHeader>
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
            <div
                data-testid="procurement-responsibility-rules"
                className="contents"
            >
                <ListWorkSurface
                    ariaLabel="采购责任规则列表"
                    toolbarClassName="pt-0 pb-3"
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
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                        title="还没有采购责任规则"
                                        description="请先配置默认负责人池；需要询价专人处理时，新增询价规则并选择负责人。"
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
            </div>
            {canManage ? (
                <RuleDialog
                    onRefreshCategories={() =>
                        categoriesQuery.refetch({ throwOnError: true })
                    }
                    onRefreshOwners={() =>
                        adminsQuery.refetch({ throwOnError: true })
                    }
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
