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
import type { PaginationState } from "@tanstack/react-table"
import { z } from "zod"

import {
    BusinessFailureState,
    ListDataTable,
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    ListWorkspacePage,
    listWorkspaceStyles as styles,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
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
import {
    CustomerSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    describeFinanceResponsibilityRule,
    useFinanceResponsibilityColumns,
} from "@/features/finance-responsibilities/hooks/use-finance-responsibility-columns"
import {
    useFinanceResponsibilityOwnerOptionsQuery,
    useFinanceResponsibilityRulesQuery,
    useSaveFinanceResponsibilityRuleMutation,
} from "@/features/finance-responsibilities/queries"
import {
    FINANCE_OPERATION_LABEL,
    FINANCE_SCOPE_LABEL,
    type FinanceResponsibilityOperation,
    type FinanceResponsibilityOwnerOption,
    type FinanceResponsibilityRule,
} from "@/features/finance-responsibilities/types"
import { getErrorMessage } from "@/lib/api/errors"
import { useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

const TITLE = "财务责任配置"
const LIST_LABEL = "财务责任规则列表"
const PREFIX = "finance-responsibilities"
const TABLE_ID = `${PREFIX}-rules-table`
const TOOLBAR_ID = `${PREFIX}-toolbar`
const CREATE_ID = `${PREFIX}-create`
const EMPTY_CREATE_ID = `${PREFIX}-empty-create`
const PAGE_SIZES = [20, 50, 100]
const DEFAULT_PAGE_SIZE = 50
const OPERATION_VALUES = ["SUPPLIER_PAYMENT", "SALES_INVOICE"] as const
const SCOPE_VALUES = ["COUNTERPARTY", "DEFAULT"] as const
const OPERATION_OPTIONS = OPERATION_VALUES.map((value) => ({
    value,
    label: FINANCE_OPERATION_LABEL[value],
}))
const STATUS_OPTIONS = [
    { value: "active", label: "已启用" },
    { value: "disabled", label: "已停用" },
]
const EMPTY_FILTERS = { q: "", operation: null, status: null }
const EMPTY_RULES: readonly FinanceResponsibilityRule[] = []
const COLUMN_ORDER = [
    "operation",
    "scope",
    "counterparty",
    "owner",
    "strategy",
    "status",
    "actions",
]
const COLUMN_VISIBILITY = { counterparty: false }
const COLUMN_PINNING = { left: ["operation"], right: ["actions"] }
const READ_ONLY_COLUMN_PINNING = { left: ["operation"] }

type RuleFilters = {
    q: string
    operation: string | null
    status: string | null
}

function writeListUrl(patch: Record<string, string | null>) {
    const url = new URL(window.location.href)
    for (const [key, value] of Object.entries(patch)) {
        if (value) url.searchParams.set(key, value)
        else url.searchParams.delete(key)
    }
    if (url.href !== window.location.href) {
        window.history.replaceState(null, "", url)
    }
}

function useRuleFilters() {
    const params = useSearchParams()
    const q = params.get("q")?.trim() ?? ""
    const operation =
        OPERATION_OPTIONS.find(({ value }) => value === params.get("operation"))
            ?.value ?? null
    const status =
        STATUS_OPTIONS.find(({ value }) => value === params.get("status"))
            ?.value ?? null
    const applied = React.useMemo<RuleFilters>(
        () => ({ q, operation, status }),
        [q, operation, status],
    )
    const [editingFilters, setEditingFilters] = React.useState({
        applied,
        draft: applied,
    })
    if (editingFilters.applied !== applied) {
        setEditingFilters({ applied, draft: applied })
    }
    const draft =
        editingFilters.applied === applied ? editingFilters.draft : applied
    const hasPendingChanges =
        draft.q.trim() !== q ||
        draft.operation !== operation ||
        draft.status !== status
    const setDraft = (patch: Partial<RuleFilters>) => {
        setEditingFilters((current) => ({
            applied,
            draft: { ...current.draft, ...patch },
        }))
    }
    const apply = (filters: RuleFilters) => {
        const next = { ...filters, q: filters.q.trim() }
        setEditingFilters({ applied, draft: next })
        writeListUrl({ ...next, page: null })
    }
    const page = Number(params.get("page"))
    const pageSize = Number(params.get("pageSize"))

    return {
        applied,
        draft,
        setDraft,
        apply,
        hasPendingChanges,
        pageIndex: Number.isSafeInteger(page) && page > 0 ? page - 1 : 0,
        pageSize: PAGE_SIZES.includes(pageSize) ? pageSize : DEFAULT_PAGE_SIZE,
    }
}

function matchesRule(rule: FinanceResponsibilityRule, filters: RuleFilters) {
    if (filters.operation && rule.operation !== filters.operation) return false
    if (filters.status && rule.enabled !== (filters.status === "active"))
        return false
    const query = filters.q.toLowerCase()
    return (
        !query ||
        [rule.counterpartyNo, ...rule.ownerNames].some((value) =>
            value?.toLowerCase().includes(query),
        )
    )
}

const ruleFormSchema = z
    .object({
        operation: z.enum(OPERATION_VALUES),
        scope: z.enum(SCOPE_VALUES),
        counterpartyId: z.string(),
        ownerUserIds: ownerPoolSchema,
        strategy: assignmentStrategySchema,
        enabled: z.boolean(),
    })
    .superRefine((value, context) => {
        if (value.scope === "COUNTERPARTY" && !value.counterpartyId) {
            context.addIssue({
                code: "custom",
                path: ["counterpartyId"],
                message:
                    value.operation === "SUPPLIER_PAYMENT"
                        ? "请选择供应商"
                        : "请选择客户",
            })
        }
    })

type RuleFormValue = z.input<typeof ruleFormSchema>

function defaultValues(rule?: FinanceResponsibilityRule): RuleFormValue {
    return {
        operation: rule?.operation ?? "SUPPLIER_PAYMENT",
        scope: rule?.scope ?? "COUNTERPARTY",
        counterpartyId: rule?.counterpartyId ?? "",
        ownerUserIds: [...(rule?.ownerUserIds ?? [])],
        strategy: rule?.strategy ?? "ROUND_ROBIN",
        enabled: rule?.enabled ?? true,
    }
}

function eligibleOwners(
    owners: readonly FinanceResponsibilityOwnerOption[],
    operation: FinanceResponsibilityOperation,
) {
    return owners
        .filter((owner) => {
            if (operation === "SUPPLIER_PAYMENT") {
                return owner.supplierPaymentEligible
            }
            if (operation === "SALES_INVOICE") {
                return owner.salesInvoiceEligible
            }
            return owner.salesInvoiceEligible
        })
        .map((owner) => ({
            value: owner.userId,
            label: `${owner.displayName} · ${owner.account}`,
        }))
}

function RuleDialog({
    open,
    onRefreshOwners,
    onOpenChange,
    finalFocus,
    target,
    owners,
    ownersReady,
}: {
    open: boolean
    onRefreshOwners: () => Promise<unknown>
    onOpenChange: (open: boolean) => void
    finalFocus: () => HTMLElement | null
    target?: FinanceResponsibilityRule
    owners: readonly FinanceResponsibilityOwnerOption[]
    ownersReady: boolean
}) {
    const mutation = useSaveFinanceResponsibilityRuleMutation()
    const form = useAppForm({
        defaultValues: defaultValues(target),
        validators: { onChange: ruleFormSchema },
        onSubmit: async ({ value }) => {
            try {
                await mutation.mutateAsync({
                    id: target?.id,
                    operation: value.operation,
                    scope: value.scope,
                    counterpartyId:
                        value.scope === "COUNTERPARTY"
                            ? value.counterpartyId
                            : undefined,
                    ownerUserIds: value.ownerUserIds,
                    strategy: value.strategy,
                    enabled: value.enabled,
                    expectedVersion: target?.version,
                })
                toast.add({
                    title: target ? "财务责任规则已更新" : "财务责任规则已新增",
                    description:
                        "新形成的付款或开票任务使用最新规则；已有任务负责人保持不变。",
                    type: "success",
                    timeout: 4500,
                })
                onOpenChange(false)
            } catch (error) {
                applyApiFormError(form, error)
            }
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-xl"
                closeButtonId="finance-responsibilities-rule-dialog-close"
                finalFocus={finalFocus}
            >
                <DialogHeader>
                    <DialogTitle>
                        {target ? "编辑财务责任规则" : "新增财务责任规则"}
                    </DialogTitle>
                    <DialogDescription>
                        指定客户/供应商规则优先于同业务的默认经办人组。保存只影响新任务；已有任务请在工作台转交。
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
                        <form.AppField name="operation">
                            {(field) => (
                                <field.SelectField
                                    id="finance-responsibilities-rule-operation"
                                    label="业务操作"
                                    options={OPERATION_OPTIONS}
                                    allowClear={false}
                                    required
                                    onValueChange={() => {
                                        form.setFieldValue("counterpartyId", "")
                                        form.setFieldValue("ownerUserIds", [])
                                    }}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="scope">
                            {(field) => (
                                <field.SelectField
                                    id="finance-responsibilities-rule-scope"
                                    label="适用范围"
                                    options={SCOPE_VALUES.map((value) => ({
                                        value,
                                        label: FINANCE_SCOPE_LABEL[value],
                                    }))}
                                    description="每项业务最多启用一条默认规则；指定客户/供应商可覆盖默认经办人组。"
                                    allowClear={false}
                                    required
                                    onValueChange={() =>
                                        form.setFieldValue("counterpartyId", "")
                                    }
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => ({
                                operation: state.values.operation,
                                scope: state.values.scope,
                            })}
                        >
                            {({ operation, scope }) => (
                                <>
                                    {scope === "COUNTERPARTY" ? (
                                        <form.AppField name="counterpartyId">
                                            {(field) => (
                                                <Field>
                                                    <FieldLabel htmlFor="finance-responsibilities-rule-counterparty">
                                                        {operation ===
                                                        "SUPPLIER_PAYMENT"
                                                            ? "供应商"
                                                            : "客户"}
                                                        <span className="text-destructive">
                                                            *
                                                        </span>
                                                    </FieldLabel>
                                                    {operation ===
                                                    "SUPPLIER_PAYMENT" ? (
                                                        <SupplierSearchCombobox
                                                            id="finance-responsibilities-rule-counterparty"
                                                            purpose="form"
                                                            value={
                                                                field.state
                                                                    .value ||
                                                                undefined
                                                            }
                                                            onValueChange={(
                                                                id,
                                                            ) =>
                                                                field.handleChange(
                                                                    id ?? "",
                                                                )
                                                            }
                                                            placeholder="搜索供应商编号或名称"
                                                        />
                                                    ) : (
                                                        <CustomerSearchCombobox
                                                            id="finance-responsibilities-rule-counterparty"
                                                            purpose="form"
                                                            scope="all_authorized"
                                                            value={
                                                                field.state
                                                                    .value ||
                                                                undefined
                                                            }
                                                            onValueChange={(
                                                                id,
                                                            ) =>
                                                                field.handleChange(
                                                                    id ?? "",
                                                                )
                                                            }
                                                            placeholder="搜索客户编号或名称"
                                                        />
                                                    )}
                                                </Field>
                                            )}
                                        </form.AppField>
                                    ) : null}
                                    <form.AppField name="ownerUserIds">
                                        {() => (
                                            <OwnerPoolField
                                                id="finance-responsibilities-rule-owner"
                                                options={eligibleOwners(
                                                    owners,
                                                    operation,
                                                )}
                                                selectedNames={Object.fromEntries(
                                                    (
                                                        target?.ownerUserIds ??
                                                        []
                                                    ).map((id, index) => [
                                                        id,
                                                        target?.ownerNames[
                                                            index
                                                        ] ?? "已选负责人",
                                                    ]),
                                                )}
                                                onRefresh={onRefreshOwners}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="strategy">
                                        {() => (
                                            <AssignmentStrategyField id="finance-responsibilities-rule-strategy" />
                                        )}
                                    </form.AppField>
                                </>
                            )}
                        </form.Subscribe>
                        <form.AppField name="enabled">
                            {(field) => (
                                <Field orientation="horizontal">
                                    <div className="flex-1">
                                        <FieldLabel htmlFor="finance-responsibilities-rule-enabled">
                                            启用规则
                                        </FieldLabel>
                                        <FieldDescription>
                                            停用后，新财务待办将不再按此规则分配。
                                        </FieldDescription>
                                    </div>
                                    <Switch
                                        id="finance-responsibilities-rule-enabled"
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
                            id="finance-responsibilities-rule-cancel"
                            render={<Button type="button" variant="outline" />}
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
                                        id="finance-responsibilities-rule-submit"
                                        type="submit"
                                        data-testid="finance-responsibility-save"
                                        disabled={
                                            !canSubmit ||
                                            pending ||
                                            !ownersReady
                                        }
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

export function FinanceResponsibilityRulesPage() {
    const profileQuery = useAccountProfileQuery()

    if (profileQuery.isPending)
        return <ListWorkspacePage title={TITLE} loading />
    if (profileQuery.isError) {
        return (
            <ListWorkspacePage title={TITLE}>
                <BusinessFailureState
                    kind="system"
                    title="权限信息加载失败"
                    description={getErrorMessage(
                        profileQuery.error,
                        "暂时无法核对财务责任配置权限。",
                    )}
                    action={
                        <Button
                            id="finance-responsibilities-profile-retry"
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

    const permissions = profileQuery.data?.permissions
    if (!hasPermission(permissions, "finance_responsibility:list")) {
        return (
            <ListWorkspacePage title={TITLE}>
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看财务责任配置。"
                />
            </ListWorkspacePage>
        )
    }

    return (
        <FinanceResponsibilityRuleList
            canManage={hasPermission(
                permissions,
                "finance_responsibility:manage",
            )}
        />
    )
}

function FinanceResponsibilityRuleList({ canManage }: { canManage: boolean }) {
    const rulesQuery = useFinanceResponsibilityRulesQuery()
    const ownersQuery = useFinanceResponsibilityOwnerOptionsQuery(canManage)
    const [dialogOpen, setDialogOpen] = React.useState(false)
    const [editing, setEditing] = React.useState<FinanceResponsibilityRule>()
    const [dialogSession, startDialogSession] = React.useReducer(
        (session: number) => session + 1,
        0,
    )
    const searchInputRef = React.useRef<HTMLInputElement>(null)
    const createTriggerId = React.useRef(CREATE_ID)
    useSearchHotkey(searchInputRef)

    const filters = useRuleFilters()
    const rows = React.useMemo(
        () =>
            (rulesQuery.data ?? EMPTY_RULES).filter((rule) =>
                matchesRule(rule, filters.applied),
            ),
        [rulesQuery.data, filters.applied],
    )
    const { pageSize } = filters
    const pageIndex = Math.min(
        filters.pageIndex,
        Math.max(0, Math.ceil(rows.length / pageSize) - 1),
    )
    const pageRows = React.useMemo(
        () => rows.slice(pageIndex * pageSize, (pageIndex + 1) * pageSize),
        [rows, pageIndex, pageSize],
    )
    const loadFailed = rulesQuery.isError || rulesQuery.data === undefined
    const ownerUnavailable = ownersQuery.isPending || ownersQuery.isError
    const ownerDisabledReason = ownersQuery.isError
        ? "负责人候选加载失败，可重试后再编辑。"
        : ownersQuery.isPending
          ? "负责人候选加载完成后可编辑。"
          : undefined
    const openCreate = (triggerId: string) => {
        createTriggerId.current = triggerId
        setEditing(undefined)
        startDialogSession()
        setDialogOpen(true)
    }
    const openEdit = React.useCallback((rule: FinanceResponsibilityRule) => {
        setEditing(rule)
        startDialogSession()
        setDialogOpen(true)
    }, [])
    const columns = useFinanceResponsibilityColumns({
        canManage,
        ownerDisabledReason,
        onEdit: openEdit,
    })
    const finalFocus = () => {
        if (editing) {
            return (
                document
                    .getElementById(TABLE_ID)
                    ?.querySelector<HTMLElement>(
                        `[data-row-id="${CSS.escape(editing.id)}"][tabindex]`,
                    ) ?? searchInputRef.current
            )
        }
        const trigger = document.getElementById(createTriggerId.current)
        const create = document.getElementById(CREATE_ID)
        if (trigger && !trigger.matches(":disabled")) return trigger
        if (create && !create.matches(":disabled")) return create
        return searchInputRef.current
    }
    const changePagination = (next: PaginationState) => {
        const nextPage = next.pageSize === pageSize ? next.pageIndex : 0
        writeListUrl({
            page: nextPage > 0 ? String(nextPage + 1) : null,
            pageSize:
                next.pageSize === DEFAULT_PAGE_SIZE
                    ? null
                    : String(next.pageSize),
        })
    }

    if (rulesQuery.isPending) return <ListWorkspacePage title={TITLE} loading />

    return (
        <PageScaffold
            density="compact"
            className={styles.page}
            data-testid="finance-responsibility-rules"
        >
            <ListWorkspaceHeader title={TITLE}>
                {canManage ? (
                    <Button
                        id={CREATE_ID}
                        type="button"
                        size="sm"
                        variant={
                            filters.hasPendingChanges ? "ghost" : "default"
                        }
                        className={
                            filters.hasPendingChanges
                                ? styles.quietButton
                                : undefined
                        }
                        data-testid="finance-responsibility-create"
                        disabled={ownerUnavailable}
                        title={ownerDisabledReason}
                        onClick={() => openCreate(CREATE_ID)}
                    >
                        <PlusIcon data-icon="inline-start" />
                        新增规则
                    </Button>
                ) : null}
            </ListWorkspaceHeader>
            {canManage && ownersQuery.isError ? (
                <BusinessFailureState
                    kind="system"
                    title="负责人候选加载失败"
                    description={getErrorMessage(
                        ownersQuery.error,
                        "暂时无法读取具备付款或开票权限的账号，当前不能编辑规则。",
                    )}
                    action={
                        <Button
                            id="finance-responsibilities-owner-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void ownersQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            ) : canManage && ownersQuery.isPending ? (
                <Alert variant="info">
                    <AlertTitle>负责人候选加载中</AlertTitle>
                    <AlertDescription>
                        加载完成后可新增或编辑财务责任规则。
                    </AlertDescription>
                </Alert>
            ) : null}
            {canManage ? (
                <RuleDialog
                    key={dialogSession}
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    finalFocus={finalFocus}
                    target={editing}
                    onRefreshOwners={() =>
                        ownersQuery.refetch({ throwOnError: true })
                    }
                    owners={ownersQuery.data ?? []}
                    ownersReady={!ownerUnavailable}
                />
            ) : null}
            <ListWorkSurface
                ariaLabel={LIST_LABEL}
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix={TOOLBAR_ID}
                        formAriaLabel="财务责任规则查询"
                        morePresentation="popover"
                        search={
                            <ListSearchField
                                id={`${TOOLBAR_ID}-search-input`}
                                searchInputRef={searchInputRef}
                                value={filters.draft.q}
                                onChange={(q) => filters.setDraft({ q })}
                                placeholder="搜索客户/供应商编号或经办人"
                                aria-label="搜索客户/供应商编号或经办人"
                            />
                        }
                        primaryFilters={
                            <>
                                <OptionCombobox
                                    id={`${TOOLBAR_ID}-operation`}
                                    className="w-40 max-w-full min-w-0"
                                    filterLabel="业务"
                                    aria-label="业务操作"
                                    value={filters.draft.operation}
                                    options={OPERATION_OPTIONS}
                                    placeholder="全部"
                                    onValueChange={(operation) =>
                                        filters.setDraft({ operation })
                                    }
                                />
                                <OptionCombobox
                                    id={`${TOOLBAR_ID}-status`}
                                    className="w-32 max-w-full min-w-0"
                                    filterLabel="状态"
                                    aria-label="规则状态"
                                    value={filters.draft.status}
                                    options={STATUS_OPTIONS}
                                    placeholder="全部"
                                    onValueChange={(status) =>
                                        filters.setDraft({ status })
                                    }
                                />
                            </>
                        }
                        resultStatus={
                            loadFailed
                                ? "查询未完成"
                                : rulesQuery.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        hasPendingChanges={filters.hasPendingChanges}
                        onSubmit={() => {
                            filters.apply(filters.draft)
                            if (!filters.hasPendingChanges || loadFailed)
                                void rulesQuery.refetch()
                        }}
                    />
                }
                tableClassName="[&_[data-column-id=actions]]:w-24 [&_[data-column-id=actions]]:min-w-20 max-md:[&_[data-column-id=operation]]:w-table-column-default max-md:[&_[data-column-id=operation]]:min-w-table-column-default-min"
                table={
                    <ListDataTable
                        id={TABLE_ID}
                        caption={LIST_LABEL}
                        entityName="财务责任规则"
                        data={pageRows}
                        columns={columns}
                        defaultColumnOrder={COLUMN_ORDER}
                        defaultColumnVisibility={COLUMN_VISIBILITY}
                        defaultColumnPinning={
                            canManage
                                ? COLUMN_PINNING
                                : READ_ONLY_COLUMN_PINNING
                        }
                        getRowId={(row) => row.id}
                        rowLabel={describeFinanceResponsibilityRule}
                        rowCount={rows.length}
                        pagination={{ pageIndex, pageSize }}
                        onPaginationChange={changePagination}
                        pageSizeOptions={PAGE_SIZES}
                        showToolbarSummary={false}
                        loading={rulesQuery.isFetching}
                        isError={loadFailed}
                        error={rulesQuery.error}
                        onRetry={() => void rulesQuery.refetch()}
                        retryButtonId="finance-responsibilities-rules-retry"
                        hasActiveFilters={Boolean(
                            filters.applied.q ||
                            filters.applied.operation ||
                            filters.applied.status,
                        )}
                        onClearFilters={() => {
                            filters.apply(EMPTY_FILTERS)
                            searchInputRef.current?.focus()
                        }}
                        canCreate={canManage && !ownerUnavailable}
                        onCreate={() => openCreate(EMPTY_CREATE_ID)}
                        createButtonId={EMPTY_CREATE_ID}
                        createLabel="新增规则"
                        emptyCopy={{
                            emptyDescription: canManage
                                ? "请先分别配置供应商付款和销项开票的默认经办人组。"
                                : undefined,
                        }}
                        onRowOpen={
                            canManage && !ownerUnavailable
                                ? openEdit
                                : undefined
                        }
                        highlightedRowId={dialogOpen ? editing?.id : undefined}
                    />
                }
            />
        </PageScaffold>
    )
}
