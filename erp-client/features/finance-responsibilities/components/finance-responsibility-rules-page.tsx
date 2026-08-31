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
import {
    CustomerSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useFinanceResponsibilityColumns } from "@/features/finance-responsibilities/hooks/use-finance-responsibility-columns"
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
import { hasPermission } from "@/lib/permissions"

const OPERATION_VALUES = [
    "SUPPLIER_PAYMENT",
    "SALES_INVOICE",
    "CARD_FUNDS_REVIEW",
] as const
const SCOPE_VALUES = ["COUNTERPARTY", "DEFAULT"] as const

const ruleFormSchema = z
    .object({
        operation: z.enum(OPERATION_VALUES),
        scope: z.enum(SCOPE_VALUES),
        counterpartyId: z.string(),
        ownerUserId: z.string().min(1, "请选择负责人"),
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
        ownerUserId: rule?.ownerUserId ?? "",
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
            return owner.cardFundsReviewEligible
        })
        .map((owner) => ({
            value: owner.userId,
            label: `${owner.displayName} · ${owner.account}`,
        }))
}

function RuleDialog({
    open,
    onOpenChange,
    target,
    owners,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target?: FinanceResponsibilityRule
    owners: readonly FinanceResponsibilityOwnerOption[]
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
                    ownerUserId: value.ownerUserId,
                    enabled: value.enabled,
                    expectedVersion: target?.version,
                })
                toast.add({
                    title: target ? "财务责任规则已更新" : "财务责任规则已新增",
                    description:
                        "新形成的付款、开票或票款复核任务使用最新规则；已有任务负责人保持不变。",
                    type: "success",
                    timeout: 4500,
                })
                onOpenChange(false)
            } catch (error) {
                // MutationCache 统一展示业务错误，避免表单重复提示。
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
                className="sm:max-w-xl"
                closeButtonId="finance-responsibilities-rule-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>
                        {target ? "编辑财务责任规则" : "新增财务责任规则"}
                    </DialogTitle>
                    <DialogDescription>
                        指定往来方规则优先于同业务的默认负责人。保存只影响新任务；已有任务请在工作台转交。
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
                                    options={OPERATION_VALUES.map((value) => ({
                                        value,
                                        label: FINANCE_OPERATION_LABEL[value],
                                    }))}
                                    allowClear={false}
                                    required
                                    onValueChange={() => {
                                        form.setFieldValue("counterpartyId", "")
                                        form.setFieldValue("ownerUserId", "")
                                    }}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="scope">
                            {(field) => (
                                <field.SelectField
                                    id="finance-responsibilities-rule-scope"
                                    label="匹配层级"
                                    options={SCOPE_VALUES.map((value) => ({
                                        value,
                                        label: FINANCE_SCOPE_LABEL[value],
                                    }))}
                                    description="每项业务最多启用一条默认规则；指定往来方可覆盖默认负责人。"
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
                                                    <FieldLabel>
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
                                    <form.AppField name="ownerUserId">
                                        {(field) => (
                                            <field.SelectField
                                                id="finance-responsibilities-rule-owner"
                                                label="负责人"
                                                options={eligibleOwners(
                                                    owners,
                                                    operation,
                                                )}
                                                placeholder="选择具备完整执行权限的账号"
                                                emptyLabel="没有符合资格的负责人，请先配置角色权限"
                                                allowClear={false}
                                                required
                                            />
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
                                            停用后不再参与新任务负责人解析。
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
                        <form.Subscribe selector={(state) => state.canSubmit}>
                            {(canSubmit) => (
                                <Button
                                    id="finance-responsibilities-rule-submit"
                                    type="submit"
                                    data-testid="finance-responsibility-save"
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

export function FinanceResponsibilityRulesPage() {
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canList = hasPermission(permissions, "finance_responsibility:list")
    const canManage = hasPermission(
        permissions,
        "finance_responsibility:manage",
    )
    const rulesQuery = useFinanceResponsibilityRulesQuery(canList)
    const ownersQuery = useFinanceResponsibilityOwnerOptionsQuery(canManage)
    const [dialogOpen, setDialogOpen] = React.useState(false)
    const [editing, setEditing] = React.useState<FinanceResponsibilityRule>()
    const columns = useFinanceResponsibilityColumns()

    if (profileQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="财务责任配置" density="compact" />
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
                <PageHeader title="财务责任配置" density="compact" />
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
                            onClick={() => void profileQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    if (!canList) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="财务责任配置" density="compact" />
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看财务责任配置。"
                />
            </PageScaffold>
        )
    }

    const rows = [...(rulesQuery.data ?? [])]
    const ownerUnavailable = ownersQuery.isPending || ownersQuery.isError
    const openCreate = () => {
        setEditing(undefined)
        setDialogOpen(true)
    }
    const openEdit = (rule: FinanceResponsibilityRule) => {
        setEditing(rule)
        setDialogOpen(true)
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="财务责任配置"
                description="为供应商付款、客户销项开票和卡券票款复核指定具体负责人；指定往来方优先，默认规则兜底。"
                actions={
                    canManage ? (
                        <Button
                            id="finance-responsibilities-create"
                            type="button"
                            size="sm"
                            data-testid="finance-responsibility-create"
                            disabled={ownerUnavailable}
                            onClick={openCreate}
                        >
                            <PlusIcon data-icon="inline-start" />
                            新增规则
                        </Button>
                    ) : undefined
                }
            />
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
            <BusinessTableFrame
                data-testid="finance-responsibility-rules"
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        责任规则列表
                        <span className="font-normal text-muted-foreground">
                            {rows.length} 条
                        </span>
                    </span>
                }
                description="负责人保存时校验账号状态及完整执行权限；缺少有效规则时业务单据不能形成付款或开票任务。"
                table={
                    <DataTable
                        id="finance-responsibilities-rules-table"
                        data={rows}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        layout="flush"
                        loading={rulesQuery.isPending || rulesQuery.isFetching}
                        showPagination={false}
                        errorState={
                            rulesQuery.isError ? (
                                <BusinessFailureState
                                    kind="system"
                                    title="规则加载失败"
                                    description={getErrorMessage(
                                        rulesQuery.error,
                                        "暂时无法读取财务责任规则。",
                                    )}
                                    action={
                                        <Button
                                            id="finance-responsibilities-rules-retry"
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
                                    title="还没有财务责任规则"
                                    description="请先分别配置供应商付款和销项开票的默认负责人。"
                                    action={
                                        canManage && !ownerUnavailable ? (
                                            <Button
                                                id="finance-responsibilities-empty-create"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                onClick={openCreate}
                                            >
                                                新增规则
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowOpen={
                            canManage && !ownerUnavailable
                                ? openEdit
                                : undefined
                        }
                    />
                }
            />
            {canManage && ownersQuery.data ? (
                <RuleDialog
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    target={editing}
                    owners={ownersQuery.data}
                />
            ) : null}
        </PageScaffold>
    )
}
