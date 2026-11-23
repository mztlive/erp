"use client"

import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronDownIcon,
    CompassIcon,
    FileSignatureIcon,
    FileSpreadsheetIcon,
    PackageSearchIcon,
    ReceiptIcon,
    UsersIcon,
} from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter } from "@/lib/navigation"
import { hasAnyPermission, hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

const createActionClass = buttonVariants({
    variant: "ghost",
    size: "sm",
    className: detailWorkspaceStyles.subtleButton,
})

const archiveLinkClass = buttonVariants({
    variant: "ghost",
    size: "sm",
    className:
        "h-control-sm px-2 font-normal text-foreground/80 shadow-none hover:bg-muted/75 hover:text-foreground",
})

/**
 * 「新建销售单」已提权至销售工作台页头作为主操作，
 * 本栏承接高频次级发起动作与业务档案查阅，分为两组清晰展示。
 */
const secondaryCreateActions = [
    {
        id: "new-inquiry",
        label: "新建销售询价",
        href: "/sales/inquiries/new",
        icon: FileSpreadsheetIcon,
        permissions: ["sales_inquiry:create", "sellable_sku:list"],
    },
    {
        id: "make-proposal",
        label: "做方案（选品）",
        href: "/master-data/sellable-items",
        icon: PackageSearchIcon,
        permissions: ["sales_selection_booklet:create", "sellable_sku:list"],
    },
    {
        id: "request-invoice",
        label: "申请开票",
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

type ListOperation = { label: string; href: string; permission: string }

export function SalesWorkbenchShortcuts({
    permissions,
    listOperations,
}: {
    permissions: readonly string[]
    listOperations: readonly ListOperation[]
}) {
    const can = (permission: string) => hasPermission(permissions, permission)
    const canReadCustomers = can("customer:list")
    const canReadContracts = can("contract:list")
    const canReadContractTemplates = hasAnyPermission(permissions, [
        "contract_template:list",
        "contract_application:list",
    ])
    const availableActions = secondaryCreateActions.filter((action) =>
        action.permissions.every(can),
    )
    const availableLists = listOperations.filter((list) => can(list.permission))
    const hasArchives =
        canReadCustomers ||
        canReadContracts ||
        canReadContractTemplates ||
        availableLists.length > 0

    if (availableActions.length === 0 && !hasArchives) {
        return null
    }

    return (
        <nav
            aria-label="销售常用入口"
            className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-2 border-b border-border/60 bg-card py-2.5"
        >
            {availableActions.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5">
                    <span className="mr-1 text-xs font-medium text-muted-foreground">
                        发起业务
                    </span>
                    {availableActions.map(({ id, label, href, icon: Icon }) => (
                        <Link
                            key={id}
                            id={`sales-workbench-${id}`}
                            href={href}
                            prefetch={false}
                            className={createActionClass}
                        >
                            <Icon
                                data-icon="inline-start"
                                className="text-primary"
                                aria-hidden="true"
                            />
                            {label}
                        </Link>
                    ))}
                </div>
            )}

            {availableActions.length > 0 && hasArchives && (
                <span
                    aria-hidden="true"
                    className="hidden h-4 w-px bg-border/80 md:inline-block"
                />
            )}

            {hasArchives && (
                <div className="flex flex-wrap items-center gap-1">
                    <span className="mr-1 text-xs font-medium text-muted-foreground">
                        业务档案
                    </span>
                    {canReadCustomers && (
                        <Link
                            id="sales-workbench-customers"
                            href="/sales/customers"
                            prefetch={false}
                            className={archiveLinkClass}
                        >
                            <UsersIcon
                                data-icon="inline-start"
                                className="text-muted-foreground"
                                aria-hidden="true"
                            />
                            客户
                        </Link>
                    )}
                    {(canReadContracts || canReadContractTemplates) && (
                        <ContractShortcut
                            canReadContracts={canReadContracts}
                            canReadContractTemplates={canReadContractTemplates}
                        />
                    )}
                    {availableLists.map(({ href, label }) => (
                        <Link
                            key={href}
                            id={`sales-workbench-list-${toAutomationIdSegment(href)}`}
                            href={href}
                            prefetch={false}
                            className={cn(archiveLinkClass, "group/list")}
                        >
                            {label}
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                className="text-muted-foreground/70 transition-colors group-hover/list:text-sidebar-accent-foreground"
                                aria-hidden="true"
                            />
                        </Link>
                    ))}
                </div>
            )}
        </nav>
    )
}

export function RoleWorkbenchShortcuts({
    permissions,
    listOperations,
}: {
    permissions: readonly string[]
    listOperations: readonly ListOperation[]
}) {
    const availableLists = listOperations.filter((list) =>
        hasPermission(permissions, list.permission),
    )
    if (availableLists.length === 0) return null

    return (
        <nav
            aria-label="岗位常用入口"
            className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 bg-card py-2.5"
        >
            <span className="mr-1 inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                <CompassIcon
                    className="size-3.5 text-primary"
                    aria-hidden="true"
                />
                常用入口
            </span>
            {availableLists.map(({ href, label }) => (
                <Link
                    key={href}
                    id={`role-operation-${toAutomationIdSegment(href)}`}
                    href={href}
                    prefetch={false}
                    className={cn(createActionClass, "group/list")}
                >
                    {label}
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        className="text-muted-foreground/70 transition-colors group-hover/list:text-sidebar-accent-foreground"
                        aria-hidden="true"
                    />
                </Link>
            ))}
        </nav>
    )
}

function ContractShortcut({
    canReadContracts,
    canReadContractTemplates,
}: {
    canReadContracts: boolean
    canReadContractTemplates: boolean
}) {
    const router = useRouter()

    if (canReadContracts && canReadContractTemplates) {
        return (
            <DropdownMenu>
                <DropdownMenuTrigger
                    id="sales-workbench-contracts-trigger"
                    render={
                        <Button
                            variant="ghost"
                            size="sm"
                            className="font-normal text-foreground/85 hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground"
                        />
                    }
                >
                    <FileSignatureIcon
                        data-icon="inline-start"
                        className="text-muted-foreground"
                        aria-hidden="true"
                    />
                    合同
                    <ChevronDownIcon
                        data-icon="inline-end"
                        className="text-muted-foreground/80"
                        aria-hidden="true"
                    />
                </DropdownMenuTrigger>
                <DropdownMenuContent className="w-40">
                    <DropdownMenuItem
                        id="sales-workbench-contracts-archive"
                        onClick={() => router.push("/sales/contracts")}
                    >
                        已签合同档案
                    </DropdownMenuItem>
                    <DropdownMenuItem
                        id="sales-workbench-contracts-templates"
                        onClick={() => router.push("/sales/contract-templates")}
                    >
                        合同模板与申请
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>
        )
    }

    return (
        <Link
            id="sales-workbench-contracts"
            href={
                canReadContracts
                    ? "/sales/contracts"
                    : "/sales/contract-templates"
            }
            prefetch={false}
            className={archiveLinkClass}
        >
            <FileSignatureIcon
                data-icon="inline-start"
                className="text-muted-foreground"
                aria-hidden="true"
            />
            {canReadContracts ? "合同" : "合同模板与申请"}
        </Link>
    )
}
