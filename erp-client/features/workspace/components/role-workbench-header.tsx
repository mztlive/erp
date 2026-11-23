"use client"

import { useState } from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    CalendarDaysIcon,
    CheckIcon,
    ChevronDownIcon,
    CompassIcon,
    EllipsisIcon,
    FileSignatureIcon,
    FileSpreadsheetIcon,
    PackageSearchIcon,
    PlusIcon,
    ReceiptIcon,
    RefreshCwIcon,
    UsersIcon,
} from "lucide-react"
import { z } from "zod"
import { detailWorkspaceStyles } from "@/components/business"
import { useAppFilterForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import type { AccountProfile } from "@/features/auth/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasAnyPermission, hasPermission } from "@/lib/permissions"
import { ROLE_WORKBENCHES, type roleWorkbench } from "@/lib/role-workbenches"
import { useRouter } from "@/lib/navigation"
import { savePreference } from "../pages/role-workbench-entry"
import type { WorkspaceOverview } from "./workspace-overview"
import { WORKBENCH_PRESENTATION } from "../lib/workbench-presentation"
import { WorkbenchDataInfo, workbenchUpdatedAt } from "./workbench-data-info"

const SALES_SECONDARY_CREATE_ACTIONS = [
    {
        id: "new-inquiry",
        label: "新建销售询价",
        shortLabel: "新建询价",
        href: "/sales/inquiries/new",
        icon: FileSpreadsheetIcon,
        permissions: ["sales_inquiry:create", "sellable_sku:list"],
    },
    {
        id: "make-proposal",
        label: "做方案（选品）",
        shortLabel: "做方案",
        href: "/master-data/sellable-items",
        icon: PackageSearchIcon,
        permissions: ["sales_selection_booklet:create", "sellable_sku:list"],
    },
    {
        id: "request-invoice",
        label: "申请开票",
        shortLabel: "申请开票",
        href: "/finance/customer-accounts/invoice-requests?create=1",
        icon: ReceiptIcon,
        permissions: [
            "sales_invoice_request:submit",
            "sales_invoice_request:list",
            "sales_order:list",
            "sales_order:detail",
            "receivable_account:list",
        ],
    },
] as const

const PERIOD_OPTIONS = [
    { value: "month-to-date", label: "本月迄今" },
    { value: "last-month", label: "上月" },
    { value: "quarter-to-date", label: "本季迄今" },
    { value: "custom", label: "自定义期间" },
] as const

const dateSchema = z.iso.date("请选择有效日期")
const rangeSchema = z
    .object({ from: dateSchema, to: dateSchema })
    .refine((value) => value.from <= value.to, {
        message: "结束日不得早于开始日",
        path: ["to"],
    })

export function RoleWorkbenchHeader({
    template,
    profile,
    periodQuery,
    onRefresh,
    refreshing,
    urgentCount = 0,
}: {
    template: NonNullable<ReturnType<typeof roleWorkbench>>
    profile: AccountProfile
    periodQuery: string
    onRefresh: () => void
    refreshing: boolean
    urgentCount?: number
}) {
    const router = useRouter()
    const [saved, setSaved] = useState<boolean | null>(null)
    const permissions = profile.permissions
    const can = (permission: string) => hasPermission(permissions, permission)
    const workbenches = ROLE_WORKBENCHES.filter((item) =>
        profile.workbenches?.includes(item.id),
    )
    const isSalesWorkbench = template.id === "WB-01"
    const showSalesPrimaryCreate = isSalesWorkbench && can("sales_order:create")

    const availableSalesCreate = isSalesWorkbench
        ? SALES_SECONDARY_CREATE_ACTIONS.filter((action) =>
              action.permissions.every(can),
          )
        : []
    const inlineSalesAction = availableSalesCreate[0]
    const dropdownSalesCreate = availableSalesCreate.slice(1)

    const availableLists = template.operations.filter((item) =>
        can(item.permission),
    )
    const inlineRoleOperations = isSalesWorkbench
        ? []
        : availableLists.slice(0, 2)
    const dropdownRoleOperations = isSalesWorkbench
        ? availableLists
        : availableLists.slice(2)

    const canReadCustomers = isSalesWorkbench && can("customer:list")
    const canReadContracts = isSalesWorkbench && can("contract:list")
    const canReadContractTemplates =
        isSalesWorkbench &&
        hasAnyPermission(permissions, [
            "contract_template:list",
            "contract_application:list",
        ])

    const hasQuickArchives =
        canReadCustomers ||
        canReadContracts ||
        canReadContractTemplates ||
        dropdownRoleOperations.length > 0
    const hasQuickDropdown = dropdownSalesCreate.length > 0 || hasQuickArchives

    return (
        <header className="sticky top-0 z-20 flex min-h-12 shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-border/60 bg-card py-2">
            <div className="flex min-w-0 items-center gap-2.5">
                <h1
                    className={detailWorkspaceStyles.title}
                    title={WORKBENCH_PRESENTATION[template.id].description}
                >
                    {template.name}
                </h1>
                {workbenches.length > 1 && (
                    <Select
                        value={template.id}
                        onValueChange={(value) => {
                            if (value && value !== template.id) {
                                router.push(
                                    `/workspace/roles/${value}?${periodQuery}`,
                                )
                            }
                        }}
                    >
                        <SelectTrigger
                            id="role-workbench-switch"
                            size="sm"
                            aria-label="切换岗位"
                            className="border-transparent bg-muted/65 text-foreground/85 hover:bg-muted"
                        >
                            <SelectValue>
                                {workbenches.find(
                                    (item) => item.id === template.id,
                                )?.name ?? template.name}
                            </SelectValue>
                        </SelectTrigger>
                        <SelectContent
                            align="start"
                            alignItemWithTrigger={false}
                        >
                            {workbenches.map((item) => (
                                <SelectItem
                                    key={item.id}
                                    value={item.id}
                                    idPrefix="role-workbench-switch"
                                >
                                    {item.name}
                                </SelectItem>
                            ))}
                        </SelectContent>
                    </Select>
                )}
            </div>
            <div className={detailWorkspaceStyles.headerActions}>
                {showSalesPrimaryCreate && (
                    <Button
                        id="sales-workbench-new-order"
                        size="sm"
                        className="px-3 shadow-none"
                        render={
                            <Link
                                href="/sales/orders?mode=create"
                                prefetch={false}
                            />
                        }
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新建销售单
                    </Button>
                )}
                {inlineSalesAction && (
                    <Button
                        id={`sales-workbench-${inlineSalesAction.id}`}
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        render={
                            <Link
                                href={inlineSalesAction.href}
                                prefetch={false}
                            />
                        }
                    >
                        <inlineSalesAction.icon
                            data-icon="inline-start"
                            className="text-primary"
                            aria-hidden="true"
                        />
                        {inlineSalesAction.shortLabel}
                    </Button>
                )}
                {inlineRoleOperations.map(({ href, label }) => (
                    <Button
                        key={href}
                        id={`role-operation-${toAutomationIdSegment(href)}`}
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        render={<Link href={href} prefetch={false} />}
                    >
                        {label}
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            className="text-muted-foreground/70"
                            aria-hidden="true"
                        />
                    </Button>
                ))}
                {hasQuickDropdown && (
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id="role-workbench-quick-actions"
                            render={
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                />
                            }
                        >
                            <CompassIcon
                                data-icon="inline-start"
                                className="text-muted-foreground"
                                aria-hidden="true"
                            />
                            快捷入口
                            <ChevronDownIcon
                                data-icon="inline-end"
                                className="text-muted-foreground/80"
                                aria-hidden="true"
                            />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-48">
                            {dropdownSalesCreate.length > 0 && (
                                <DropdownMenuGroup>
                                    <DropdownMenuLabel>
                                        发起业务
                                    </DropdownMenuLabel>
                                    {dropdownSalesCreate.map(
                                        ({ id, label, href, icon: Icon }) => (
                                            <DropdownMenuItem
                                                key={id}
                                                id={`sales-workbench-${id}`}
                                                onClick={() =>
                                                    router.push(href)
                                                }
                                            >
                                                <Icon
                                                    className="text-primary"
                                                    aria-hidden="true"
                                                />
                                                {label}
                                            </DropdownMenuItem>
                                        ),
                                    )}
                                </DropdownMenuGroup>
                            )}
                            {dropdownSalesCreate.length > 0 &&
                                hasQuickArchives && <DropdownMenuSeparator />}
                            {hasQuickArchives && (
                                <DropdownMenuGroup>
                                    <DropdownMenuLabel>
                                        {isSalesWorkbench
                                            ? "业务档案"
                                            : "常用入口"}
                                    </DropdownMenuLabel>
                                    {canReadCustomers && (
                                        <DropdownMenuItem
                                            id="sales-workbench-customers"
                                            onClick={() =>
                                                router.push("/sales/customers")
                                            }
                                        >
                                            <UsersIcon
                                                className="text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            客户
                                        </DropdownMenuItem>
                                    )}
                                    {canReadContracts && (
                                        <DropdownMenuItem
                                            id="sales-workbench-contracts-archive"
                                            onClick={() =>
                                                router.push("/sales/contracts")
                                            }
                                        >
                                            <FileSignatureIcon
                                                className="text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            已签合同档案
                                        </DropdownMenuItem>
                                    )}
                                    {canReadContractTemplates && (
                                        <DropdownMenuItem
                                            id="sales-workbench-contracts-templates"
                                            onClick={() =>
                                                router.push(
                                                    "/sales/contract-templates",
                                                )
                                            }
                                        >
                                            <FileSignatureIcon
                                                className="text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            合同模板与申请
                                        </DropdownMenuItem>
                                    )}
                                    {dropdownRoleOperations.map(
                                        ({ href, label }) => (
                                            <DropdownMenuItem
                                                key={href}
                                                id={
                                                    isSalesWorkbench
                                                        ? `sales-workbench-list-${toAutomationIdSegment(href)}`
                                                        : `role-operation-${toAutomationIdSegment(href)}`
                                                }
                                                onClick={() =>
                                                    router.push(href)
                                                }
                                            >
                                                <ArrowUpRightIcon
                                                    className="text-muted-foreground"
                                                    aria-hidden="true"
                                                />
                                                {label}
                                            </DropdownMenuItem>
                                        ),
                                    )}
                                </DropdownMenuGroup>
                            )}
                        </DropdownMenuContent>
                    </DropdownMenu>
                )}
                <Button
                    id="role-workbench-all-todos"
                    variant={showSalesPrimaryCreate ? "ghost" : "default"}
                    size="sm"
                    className={
                        showSalesPrimaryCreate
                            ? detailWorkspaceStyles.subtleButton
                            : "px-3 shadow-none"
                    }
                    render={<Link href="/workspace/tasks" />}
                >
                    待我处理
                    {urgentCount > 0 ? (
                        <span className="num rounded-full bg-destructive-soft px-1.5 text-tiny font-semibold leading-4 text-destructive-soft-foreground">
                            {urgentCount}
                        </span>
                    ) : null}
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                    />
                </Button>
                <Button
                    id="role-workbench-refresh"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onRefresh}
                    disabled={refreshing}
                >
                    <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                        className={refreshing ? "animate-spin" : undefined}
                    />
                    {refreshing ? "更新中" : "刷新"}
                </Button>
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="role-workbench-more"
                        aria-label="更多工作台设置"
                        render={
                            <Button
                                variant="ghost"
                                size="icon-sm"
                                className={detailWorkspaceStyles.quietButton}
                            />
                        }
                    >
                        <EllipsisIcon aria-hidden="true" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-44">
                        <DropdownMenuItem
                            id="role-workbench-default"
                            onClick={() =>
                                setSaved(
                                    savePreference(
                                        profile.userid,
                                        template.id,
                                        true,
                                    ),
                                )
                            }
                        >
                            {saved === true && <CheckIcon aria-hidden="true" />}
                            设为默认首页
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
                {saved !== null && (
                    <span
                        role="status"
                        className={
                            saved
                                ? "text-xs text-muted-foreground"
                                : "text-xs text-destructive"
                        }
                    >
                        {saved ? "已设为默认首页" : "保存失败，请重试"}
                    </span>
                )}
            </div>
        </header>
    )
}

export function RoleWorkbenchPeriod({
    workbenchId = "WB-01",
    overview,
    custom,
}: {
    workbenchId?: "WB-01" | "WB-05"
    overview: WorkspaceOverview
    custom?: { from: string; to: string }
}) {
    const router = useRouter()
    const [open, setOpen] = useState(false)
    const isManagement = workbenchId === "WB-05"
    const prefix = isManagement ? "management" : "sales"
    const form = useAppFilterForm({
        defaultValues: custom ?? overview.period,
        validators: { onChange: rangeSchema, onSubmit: rangeSchema },
        onSubmit: ({ value }) => {
            router.replace(
                `/workspace/roles/${workbenchId}?from=${value.from}&to=${value.to}`,
            )
            setOpen(false)
        },
    })
    return (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
            <div className="flex items-center gap-1.5">
                <h2 className="text-section-title font-semibold tracking-tight">
                    业务概览
                </h2>
                <WorkbenchDataInfo
                    id={`${prefix}-workbench-scope-info`}
                    title="业务概览"
                >
                    {isManagement ? (
                        <>
                            <p>
                                经营指标按已配置的期间口径汇总非卡券、不含税业务数据，受当前账号组织范围及金额字段权限约束。
                            </p>
                            <p>
                                期间筛选同时作用于顶部经营指标与下方经营盈亏明细；销售单阶段分布与待办时限均为截至当前存量。
                            </p>
                            <p>
                                待办只包含本人具备处理资格的待办任务；更多审批与异常处理请进入待我处理。
                            </p>
                        </>
                    ) : (
                        <>
                            <p>
                                各项业务统计分别按对应模块的授权范围展示，具体范围见指标详情。
                            </p>
                            <p>
                                期间筛选只影响新增销售单；审批中销售单、待销售处理询价与应收未收均为截至当前的数据。
                            </p>
                            <p>
                                待办只包含本人具备处理资格的待办任务，不随期间变化。
                            </p>
                        </>
                    )}
                </WorkbenchDataInfo>
                <span className="hidden text-xs text-muted-foreground sm:inline">
                    ·{" "}
                    {overview.queries.some(
                        (query) => query.isFetching && !query.isPending,
                    )
                        ? "更新中…"
                        : workbenchUpdatedAt(
                              overview.queries.map((query) =>
                                  query.isError ? undefined : query.data?.asOf,
                              ),
                          )}
                </span>
            </div>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <label htmlFor="role-workbench-period">
                    {isManagement ? "经营统计期间" : "新增单据期间"}
                </label>
                <Select
                    value={custom ? "custom" : overview.preset}
                    onValueChange={(value) => {
                        if (!value) return
                        if (value === "custom") {
                            form.reset(custom ?? overview.period)
                            setOpen(true)
                        } else {
                            router.replace(
                                `/workspace/roles/${workbenchId}?period=${value}`,
                            )
                        }
                    }}
                >
                    <SelectTrigger
                        id="role-workbench-period"
                        size="sm"
                        aria-describedby={`${prefix}-workbench-period-description`}
                        className="border-transparent bg-muted/65 text-foreground/85 hover:bg-muted"
                    >
                        <SelectValue>
                            {PERIOD_OPTIONS.find(
                                (option) =>
                                    option.value ===
                                    (custom ? "custom" : overview.preset),
                            )?.label ?? "本月迄今"}
                        </SelectValue>
                    </SelectTrigger>
                    <SelectContent align="end" alignItemWithTrigger={false}>
                        {PERIOD_OPTIONS.map((option) => (
                            <SelectItem
                                key={option.value}
                                value={option.value}
                                idPrefix="role-workbench-period"
                            >
                                {option.label}
                            </SelectItem>
                        ))}
                    </SelectContent>
                </Select>
                <Popover
                    open={open}
                    onOpenChange={(next) => {
                        if (next) form.reset(custom ?? overview.period)
                        setOpen(next)
                    }}
                >
                    <PopoverTrigger
                        id={`${prefix}-workbench-custom-period`}
                        aria-label="设置自定义统计期间"
                        render={
                            <Button
                                variant="ghost"
                                size="sm"
                                className={`num ${detailWorkspaceStyles.subtleButton}`}
                            />
                        }
                    >
                        <CalendarDaysIcon
                            data-icon="inline-start"
                            className="text-muted-foreground"
                            aria-hidden="true"
                        />
                        {overview.period.from} 至 {overview.period.to}
                    </PopoverTrigger>
                    <PopoverContent
                        align="end"
                        className="w-80 max-w-[calc(100vw-2rem)]"
                    >
                        <PopoverTitle className="text-section-title font-semibold tracking-tight">
                            {isManagement
                                ? "自定义经营统计期间"
                                : "自定义新增单据期间"}
                        </PopoverTitle>
                        <PopoverDescription className="text-xs">
                            {isManagement
                                ? "按已配置的经营分析期间口径统计，包含起止日；存量阶段与待办不受影响。"
                                : "按创建日期统计，包含起止日；当前余额与待办不受影响。"}
                        </PopoverDescription>
                        <form
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                            className="space-y-3"
                        >
                            <form.AppField name="from">
                                {(field) => (
                                    <div className="space-y-1.5">
                                        <label
                                            htmlFor="role-period-from"
                                            className="text-xs"
                                        >
                                            开始日
                                        </label>
                                        <Input
                                            id="role-period-from"
                                            type="date"
                                            value={field.state.value}
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.AppField>
                            <form.AppField name="to">
                                {(field) => (
                                    <div className="space-y-1.5">
                                        <label
                                            htmlFor="role-period-to"
                                            className="text-xs"
                                        >
                                            结束日
                                        </label>
                                        <Input
                                            id="role-period-to"
                                            type="date"
                                            value={field.state.value}
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            aria-describedby="role-period-error"
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.AppField>
                            <form.Subscribe selector={(state) => state.values}>
                                {(values) => {
                                    const valid = rangeSchema.safeParse(values)
                                    return (
                                        <>
                                            <p
                                                id="role-period-error"
                                                role="status"
                                                className="text-xs text-destructive"
                                            >
                                                {!valid.success
                                                    ? valid.error.issues[0]
                                                          ?.message
                                                    : null}
                                            </p>
                                            <Button
                                                id="role-period-apply"
                                                type="submit"
                                                size="sm"
                                                className="w-full"
                                                disabled={!valid.success}
                                            >
                                                应用期间
                                            </Button>
                                        </>
                                    )
                                }}
                            </form.Subscribe>
                        </form>
                    </PopoverContent>
                </Popover>
                <span
                    id={`${prefix}-workbench-period-description`}
                    className="sr-only"
                >
                    {isManagement
                        ? "按所选期间统计经营指标与盈亏明细，阶段分布与待办截至当前。"
                        : "仅筛选新增销售单，其他指标截至当前。"}
                </span>
            </div>
        </div>
    )
}

export const SalesWorkbenchPeriod = RoleWorkbenchPeriod
