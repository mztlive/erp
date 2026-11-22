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
    BusinessFailureState,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import {
    ListDataTable,
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    ListWorkspacePage,
    listWorkspaceFilterStatusText,
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
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    getResponsibilityRuleLabel,
    RESPONSIBILITY_RULES_LIST_ID,
    ruleColumnOrder,
    ruleColumnPinning,
    ruleColumnVisibility,
    useResponsibilityRulesColumns,
} from "@/features/procurement-responsibilities/hooks/use-responsibility-rules-columns"
import {
    useProcurementResponsibilityRulesQuery,
    useSaveProcurementResponsibilityRuleMutation,
} from "@/features/procurement-responsibilities/queries"
import {
    PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL,
    type ProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/types"
import { getErrorMessage } from "@/lib/api/errors"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { patchUrl } from "@/lib/patch-search-params"
import { hasPermission } from "@/lib/permissions"
import { useSearchDraft } from "@/lib/use-search-draft"

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

function defaultValues(
    canUseSku: boolean,
    rule?: ProcurementResponsibilityRule,
): RuleFormValue {
    return {
        ruleType: rule?.ruleType ?? (canUseSku ? "SKU" : "INQUIRY"),
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
    canUseSku,
    onOpenChange,
    finalFocus,
    target,
    categories,
    ownerOptions,
}: {
    open: boolean
    onRefreshCategories: () => Promise<unknown>
    onRefreshOwners: () => Promise<unknown>
    canUseSku: boolean
    onOpenChange: (open: boolean) => void
    finalFocus: () => HTMLElement | null
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
        defaultValues: defaultValues(canUseSku, target),
        validators: { onChange: ruleFormSchema },
        onSubmit: async ({ value }) => {
            if (value.ruleType === "SKU" && !canUseSku) return
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
        if (open) form.reset(defaultValues(canUseSku, target))
    }, [canUseSku, form, open, target])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && mutation.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                className="sm:max-w-2xl"
                closeButtonId="procurement-responsibility-rules-dialog-close"
                showCloseButton={!mutation.isPending}
                finalFocus={finalFocus}
            >
                <DialogHeader>
                    <DialogTitle>
                        {target ? "编辑采购责任规则" : "新增采购责任规则"}
                    </DialogTitle>
                    <DialogDescription>
                        每条规则配置一组负责采购员。询价规则整单优先，其余按
                        SKU、分类、商品类型、默认分派组依次匹配。
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
                                    options={RULE_TYPE_VALUES.filter(
                                        (value) => value !== "SKU" || canUseSku,
                                    ).map((value) => ({
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
                                    {canUseSku && ruleType === "SKU" ? (
                                        <form.AppField name="skuId">
                                            {(field) => (
                                                <Field>
                                                    <FieldLabel htmlFor="procurement-responsibility-rules-dialog-sku">
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
                                                    <FieldLabel htmlFor="procurement-responsibility-rules-dialog-category">
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
                                            停用后，新销售订单不再按此规则分派采购员。
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
                                    disabled={mutation.isPending}
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

const LIST_ID = RESPONSIBILITY_RULES_LIST_ID
const RULE_TYPE_OPTIONS = RULE_TYPE_VALUES.map((value) => ({
    value,
    label: PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[value],
}))
const PAGE_SIZES = [20, 50, 100]
const DEFAULT_PAGE_SIZE = 50

function useRuleListControls(
    data: readonly ProcurementResponsibilityRule[] | undefined,
) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const query = searchParams.get("q")?.trim() ?? ""
    const rawRuleType = searchParams.get("ruleType")
    const ruleType =
        RULE_TYPE_OPTIONS.find((option) => option.value === rawRuleType)
            ?.value ?? null
    const { searchDraft, setSearchDraft } = useSearchDraft(query)
    const [ruleTypeDraft, setRuleTypeDraft] = React.useState<string | null>(
        ruleType,
    )
    React.useEffect(() => setRuleTypeDraft(ruleType), [ruleType])

    const rows = React.useMemo(() => {
        const search = query.toLocaleLowerCase("zh-CN")
        return (data ?? []).filter(
            (rule) =>
                (!ruleType || rule.ruleType === ruleType) &&
                (!search ||
                    getResponsibilityRuleLabel(rule)
                        .toLocaleLowerCase("zh-CN")
                        .includes(search) ||
                    rule.ownerNames.some((name) =>
                        name.toLocaleLowerCase("zh-CN").includes(search),
                    )),
        )
    }, [data, query, ruleType])

    const pageSize =
        PAGE_SIZES.find(
            (size) => String(size) === searchParams.get("pageSize"),
        ) ?? DEFAULT_PAGE_SIZE
    const requestedPage = Number(searchParams.get("page"))
    const pageIndex =
        Number.isSafeInteger(requestedPage) && requestedPage > 0
            ? Math.min(
                  requestedPage - 1,
                  Math.max(0, Math.ceil(rows.length / pageSize) - 1),
              )
            : 0
    const updateUrl = (patch: Record<string, string | null>) =>
        patchUrl({ router, pathname, searchParams }, patch, {
            replace: true,
            scroll: false,
        })
    const hasPendingChanges =
        searchDraft.trim() !== query || ruleTypeDraft !== ruleType

    return {
        rows,
        pagination: { pageIndex, pageSize },
        changePagination: (next: { pageIndex: number; pageSize: number }) =>
            updateUrl({
                page: next.pageIndex > 0 ? String(next.pageIndex + 1) : null,
                pageSize:
                    next.pageSize === DEFAULT_PAGE_SIZE
                        ? null
                        : String(next.pageSize),
            }),
        hasActiveFilters: Boolean(query || ruleType),
        hasPendingChanges,
        searchDraft,
        setSearchDraft,
        ruleTypeDraft,
        setRuleTypeDraft,
        searchInputRef,
        resultsHeadingRef,
        lastFocusedRowId,
        applyFilters: () => {
            if (hasPendingChanges) {
                updateUrl({
                    q: searchDraft.trim() || null,
                    ruleType: ruleTypeDraft,
                    page: null,
                })
            }
        },
        clearFilters: () => {
            setSearchDraft("")
            setRuleTypeDraft(null)
            updateUrl({ q: null, ruleType: null, page: null })
            searchInputRef.current?.focus()
        },
    }
}

export function ProcurementResponsibilityRulesPage() {
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.isSuccess
        ? profileQuery.data.permissions
        : undefined
    const canList = hasPermission(
        permissions,
        "procurement_responsibility:list",
    )
    const canManage =
        hasPermission(permissions, "procurement_responsibility:manage") &&
        hasPermission(permissions, "admin:list") &&
        hasPermission(permissions, "product_category:list")
    const canUseSku = hasPermission(permissions, "sku:list")
    const rulesQuery = useProcurementResponsibilityRulesQuery(canList)
    const adminsQuery = useAdminsQuery(canManage)
    const categoriesQuery = useMasterDataListQuery(
        {
            resource: "categories",
            lifecycleStatus: "all",
            revisionTiming: "current",
        },
        canManage,
    )
    const dependenciesFailed = adminsQuery.isError || categoriesQuery.isError
    const dependenciesPending =
        adminsQuery.isPending || categoriesQuery.isPending
    const disabledReason = dependenciesFailed
        ? "负责人或分类选项加载失败，请先重试"
        : dependenciesPending
          ? "正在加载负责人和分类选项"
          : undefined
    const canSave = canManage && !disabledReason
    const list = useRuleListControls(canList ? rulesQuery.data : undefined)
    const { lastFocusedRowId, resultsHeadingRef } = list
    const createTriggerRef = React.useRef<HTMLElement | null>(null)
    const [dialogOpen, setDialogOpen] = React.useState(false)
    const [editing, setEditing] =
        React.useState<ProcurementResponsibilityRule>()
    const openCreate = (trigger: HTMLElement | null) => {
        createTriggerRef.current = trigger
        lastFocusedRowId.current = null
        setEditing(undefined)
        setDialogOpen(true)
    }
    const canEditRule = React.useCallback(
        (rule: ProcurementResponsibilityRule) =>
            canUseSku || rule.ruleType !== "SKU",
        [canUseSku],
    )
    const openEdit = React.useCallback(
        (rule: ProcurementResponsibilityRule) => {
            if (!canSave || !canEditRule(rule)) return
            lastFocusedRowId.current = rule.ruleId
            setEditing(rule)
            setDialogOpen(true)
        },
        [canSave, canEditRule, lastFocusedRowId],
    )
    const restoreDialogFocus = () => {
        const ruleId = lastFocusedRowId.current
        if (!ruleId) {
            return createTriggerRef.current?.isConnected
                ? createTriggerRef.current
                : document.getElementById(`${LIST_ID}-create`)
        }
        return (
            document.querySelector<HTMLElement>(
                `#${LIST_ID}-table [data-row-id="${CSS.escape(ruleId)}"]`,
            ) ?? resultsHeadingRef.current
        )
    }
    const columns = useResponsibilityRulesColumns({
        onEdit: canManage ? openEdit : undefined,
        canUseSku,
        editDisabledReason: disabledReason,
    })

    if (
        profileQuery.isPending ||
        (canList && rulesQuery.isPending && rulesQuery.isFetching)
    ) {
        return <ListWorkspacePage title="采购责任规则" loading />
    }

    if (profileQuery.isError) {
        return (
            <ListWorkspacePage title="采购责任规则">
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
            </ListWorkspacePage>
        )
    }

    if (!canList && !canManage) {
        return (
            <ListWorkspacePage title="采购责任规则">
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看采购责任规则。"
                />
            </ListWorkspacePage>
        )
    }

    const listFailed =
        rulesQuery.isError || (!rulesQuery.isFetching && !rulesQuery.data)
    const resultStatus =
        rulesQuery.isFetching || listFailed
            ? listWorkspaceFilterStatusText({
                  loading: rulesQuery.isFetching && !listFailed,
                  failed: listFailed,
                  noun: "采购责任规则",
              })
            : null

    return (
        <PageScaffold
            density="compact"
            className={styles.page}
            data-testid={canList ? LIST_ID : undefined}
        >
            <ListWorkspaceHeader title="采购责任规则">
                {canManage ? (
                    <Button
                        id={`${LIST_ID}-create`}
                        type="button"
                        size="sm"
                        variant={
                            canList && list.hasPendingChanges
                                ? "ghost"
                                : "default"
                        }
                        className={
                            canList && list.hasPendingChanges
                                ? styles.quietButton
                                : undefined
                        }
                        data-testid="procurement-responsibility-create"
                        disabled={!canSave}
                        title={disabledReason}
                        onClick={(event) => openCreate(event.currentTarget)}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新增规则
                    </Button>
                ) : null}
            </ListWorkspaceHeader>
            {canManage && dependenciesFailed ? (
                <BusinessFailureState
                    kind="system"
                    title="采购员或商品分类数据加载失败"
                    description={getErrorMessage(
                        adminsQuery.error ?? categoriesQuery.error,
                        "暂时无法读取采购负责人或商品分类，当前不能新增或编辑规则。",
                    )}
                    action={
                        <Button
                            id="procurement-responsibility-rules-retry-dependencies"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                void adminsQuery.refetch()
                                void categoriesQuery.refetch()
                            }}
                        >
                            重新加载
                        </Button>
                    }
                />
            ) : canManage && dependenciesPending ? (
                <Alert variant="info">
                    <AlertTitle>选项加载中</AlertTitle>
                    <AlertDescription>
                        正在加载采购负责人和商品分类，完成后可新增或编辑规则。
                    </AlertDescription>
                </Alert>
            ) : null}
            {canManage && (!editing || canEditRule(editing)) ? (
                <RuleDialog
                    canUseSku={canUseSku}
                    onRefreshCategories={() =>
                        categoriesQuery.refetch({ throwOnError: true })
                    }
                    onRefreshOwners={() =>
                        adminsQuery.refetch({ throwOnError: true })
                    }
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    finalFocus={restoreDialogFocus}
                    target={editing}
                    categories={(categoriesQuery.data?.rows ?? []).map(
                        (row) => ({
                            categoryId: row.stableId,
                            categoryCode: row.dictionaryCode ?? row.stableNo,
                            categoryName: row.name,
                            parentId: row.parentStableId,
                        }),
                    )}
                    ownerOptions={(adminsQuery.data ?? []).map((account) => ({
                        value: account.id,
                        label: `${account.name} · ${account.account}`,
                    }))}
                />
            ) : null}
            {canList ? (
                <>
                    <h2
                        ref={resultsHeadingRef}
                        id={`${LIST_ID}-results`}
                        tabIndex={-1}
                        className="sr-only outline-none"
                    >
                        采购责任规则列表
                    </h2>
                    <ListWorkSurface
                        ariaLabel="采购责任规则列表"
                        toolbar={
                            <ListWorkspaceFilterBar
                                idPrefix={`${LIST_ID}-toolbar`}
                                formAriaLabel="查询采购责任规则"
                                morePresentation="popover"
                                onSubmit={list.applyFilters}
                                hasPendingChanges={list.hasPendingChanges}
                                resultStatus={resultStatus}
                                search={
                                    <ListSearchField
                                        id={`${LIST_ID}-toolbar-search-input`}
                                        searchInputRef={list.searchInputRef}
                                        value={list.searchDraft}
                                        onChange={list.setSearchDraft}
                                        placeholder="搜索匹配范围或负责人"
                                        aria-label="搜索匹配范围或负责人"
                                    />
                                }
                                primaryFilters={
                                    <OptionCombobox
                                        id={`${LIST_ID}-toolbar-rule-type`}
                                        className="w-32 max-w-full min-w-0"
                                        filterLabel="类型"
                                        aria-label="规则类型"
                                        placeholder="全部"
                                        value={list.ruleTypeDraft}
                                        options={RULE_TYPE_OPTIONS}
                                        onValueChange={list.setRuleTypeDraft}
                                    />
                                }
                            />
                        }
                        tableClassName="[&_[data-column-id=actions]]:w-24 [&_[data-column-id=actions]]:min-w-24 max-md:[&_[data-column-id=scope]]:min-w-table-column-default-min"
                        table={
                            <ListDataTable
                                id={`${LIST_ID}-table`}
                                entityName="采购责任规则"
                                caption="采购责任规则"
                                data={list.rows}
                                columns={columns}
                                defaultColumnOrder={ruleColumnOrder}
                                defaultColumnPinning={ruleColumnPinning}
                                defaultColumnVisibility={ruleColumnVisibility}
                                getRowId={(row) => row.ruleId}
                                rowLabel={getResponsibilityRuleLabel}
                                rowCount={list.rows.length}
                                pagination={list.pagination}
                                onPaginationChange={list.changePagination}
                                pageSizeOptions={PAGE_SIZES}
                                manualPagination={false}
                                scrollable
                                showToolbarSummary={false}
                                loading={rulesQuery.isFetching}
                                isError={listFailed}
                                error={rulesQuery.error}
                                onRetry={() => void rulesQuery.refetch()}
                                retryButtonId={`${LIST_ID}-retry`}
                                hasActiveFilters={list.hasActiveFilters}
                                onClearFilters={list.clearFilters}
                                clearFiltersButtonId={`${LIST_ID}-empty-clear-filters`}
                                canCreate={canSave}
                                onCreate={() =>
                                    openCreate(
                                        document.getElementById(
                                            `${LIST_ID}-empty-create`,
                                        ),
                                    )
                                }
                                createButtonId={`${LIST_ID}-empty-create`}
                                createLabel="新增规则"
                                emptyCopy={{
                                    emptyDescription: canManage
                                        ? "请先配置默认分派组；需要询价专人处理时，新增询价规则并选择负责人。"
                                        : undefined,
                                }}
                                onRowOpen={canSave ? openEdit : undefined}
                                highlightedRowId={
                                    dialogOpen ? editing?.ruleId : undefined
                                }
                            />
                        }
                    />
                </>
            ) : null}
        </PageScaffold>
    )
}
