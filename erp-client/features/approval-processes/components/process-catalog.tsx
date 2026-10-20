"use client"

import Link from "next/link"
import {
    BanknoteIcon,
    BoxesIcon,
    FilePenLineIcon,
    FileTextIcon,
    PencilIcon,
    PlusIcon,
    ReceiptTextIcon,
    ShoppingBagIcon,
    ShoppingCartIcon,
    TicketIcon,
    TruckIcon,
    Undo2Icon,
    type LucideIcon,
} from "lucide-react"
import { TableRowActions } from "@/components/business/table-row-actions"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    configurationStatusLabel,
    configurationStatusTone,
    documentTypeLabel,
    versionLabel,
} from "../labels"
import { canPerformCatalogAction } from "../permissions"
import type {
    DefinitionAllowedAction,
    DefinitionCatalogItem,
    DocumentType,
} from "../types"

const PROCESS_ICONS: Record<DocumentType, LucideIcon> = {
    sales_order: ShoppingBagIcon,
    voucher_sales_order: TicketIcon,
    sales_change_order: FilePenLineIcon,
    purchase_order: ShoppingCartIcon,
    purchase_change_order: FilePenLineIcon,
    purchase_payment_request: BanknoteIcon,
    stock_adjustment: BoxesIcon,
    customer_receipt: ReceiptTextIcon,
    sales_invoice_request: FileTextIcon,
    supplier_payment: BanknoteIcon,
    customer_refund: Undo2Icon,
    supplier_refund: Undo2Icon,
    receipt_reversal: Undo2Icon,
    payment_reversal: Undo2Icon,
    purchase_receipt: BoxesIcon,
    delivery: TruckIcon,
    electronic_delivery: TicketIcon,
    service_fulfillment: TruckIcon,
    customer_acceptance: FileTextIcon,
    invoice: FileTextIcon,
    sales_return_case: Undo2Icon,
    purchase_return_order: Undo2Icon,
}

type CatalogProps = {
    items: readonly DefinitionCatalogItem[]
    permissions: readonly string[] | undefined
    onCreateDraft: (item: DefinitionCatalogItem) => void
    onContinueDraft: (item: DefinitionCatalogItem) => void
    id?: string
}

/** 流程目录使用卡片；无需审批的类型单独收在下方，避免混入配置入口。 */
export function ProcessCatalog({
    items,
    permissions,
    onCreateDraft,
    onContinueDraft,
    id = "governance-approval-processes-catalog",
}: CatalogProps) {
    const required = items.filter(
        (item) => item.approval_requirement === "PROCESS_REQUIRED",
    )
    const exempt = items.filter(
        (item) => item.approval_requirement === "NO_APPROVAL",
    )
    return (
        <div className="space-y-6 pb-2">
            {required.length > 0 && (
                <ul
                    aria-label="审批流程"
                    className="grid gap-3 md:grid-cols-2 xl:grid-cols-3"
                >
                    {required.map((item) => (
                        <ProcessCard
                            key={item.document_type}
                            item={item}
                            permissions={permissions}
                            onCreateDraft={onCreateDraft}
                            onContinueDraft={onContinueDraft}
                            id={id}
                        />
                    ))}
                </ul>
            )}
            {exempt.length > 0 && (
                <section
                    aria-labelledby={`${id}-exempt-title`}
                    className="space-y-3 border-t border-border/70 pt-5"
                >
                    <div className="flex items-center gap-2">
                        <h2
                            id={`${id}-exempt-title`}
                            className="text-sm font-medium text-muted-foreground"
                        >
                            无需审批
                        </h2>
                        <span className="num text-xs text-muted-foreground">
                            {exempt.length}
                        </span>
                    </div>
                    <ul
                        aria-label="无需审批的单据类型"
                        className="grid gap-3 md:grid-cols-2 xl:grid-cols-3"
                    >
                        {exempt.map((item) => {
                            const Icon = PROCESS_ICONS[item.document_type]
                            return (
                                <li
                                    key={item.document_type}
                                    data-document-type={item.document_type}
                                    className="flex min-w-0 items-center gap-2 py-1"
                                >
                                    <Icon
                                        aria-hidden="true"
                                        className="size-4 shrink-0 text-muted-foreground"
                                    />
                                    <span className="min-w-0 break-words text-sm text-muted-foreground">
                                        {documentTypeLabel(
                                            item.document_type,
                                            item.document_type_label,
                                        )}
                                    </span>
                                </li>
                            )
                        })}
                    </ul>
                </section>
            )}
        </div>
    )
}

function ProcessCard({
    item,
    permissions,
    onCreateDraft,
    onContinueDraft,
    id,
}: Omit<CatalogProps, "items" | "id"> & {
    item: DefinitionCatalogItem
    id: string
}) {
    const segment = toAutomationIdSegment(item.document_type)
    const href = `/system/approval-processes/${item.document_type}${!item.published_version && item.draft_version ? "?view=draft" : ""}`
    const label = documentTypeLabel(
        item.document_type,
        item.document_type_label,
    )
    const actions = visibleActions(item, permissions)
    const canEdit = actions.includes("REPLACE_NODES")
    const canCreate = actions.includes("CREATE_DRAFT")
    const Icon = PROCESS_ICONS[item.document_type]
    const missing = item.configuration_status === "MISSING_CONFIGURATION"
    return (
        <li
            data-document-type={item.document_type}
            data-blocked={missing ? "true" : "false"}
            className="relative flex min-w-0 items-center gap-2 rounded-lg border border-border/80 bg-card px-4 py-4 transition-colors hover:bg-muted/20 focus-within:border-primary/40"
        >
            <Link
                id={`${id}-row-${segment}-open`}
                href={href}
                className="group flex min-w-0 flex-1 items-start gap-3 rounded-sm py-1 after:absolute after:inset-0 after:rounded-lg focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring"
            >
                <Icon
                    aria-hidden="true"
                    className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                />
                <div className="min-w-0 space-y-2">
                    <h2 className="text-sm font-medium leading-5 break-words group-hover:text-primary">
                        {label}
                    </h2>
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                        {missing ? (
                            <StatusBadge
                                label={configurationStatusLabel(
                                    item.configuration_status,
                                    item.approval_requirement,
                                )}
                                tone={configurationStatusTone(
                                    item.configuration_status,
                                    item.approval_requirement,
                                )}
                            />
                        ) : (
                            <span className="num">
                                {item.published_version
                                    ? `已发布 · ${versionLabel(item.published_version)}`
                                    : "尚未发布"}
                            </span>
                        )}
                        {item.draft_version ? (
                            <span className="num text-warning-soft-foreground">
                                草稿 {item.draft_version} 待发布
                            </span>
                        ) : null}
                    </div>
                </div>
            </Link>
            <TableRowActions
                className="relative z-10 shrink-0 gap-1"
                moreId={`${id}-row-${segment}-more`}
                moreLabel={`${label}更多操作`}
                moreIconOnly
                maxInline={1}
                actions={[
                    {
                        id: `${id}-row-${segment}-continue`,
                        label: "编辑",
                        icon: PencilIcon,
                        hidden: !canEdit,
                        onClick: () => onContinueDraft(item),
                    },
                    {
                        id: `${id}-row-${segment}-create-draft`,
                        label: item.published_version ? "新建草稿" : "配置流程",
                        icon: PlusIcon,
                        hidden: canEdit || !canCreate,
                        placement: item.published_version ? "menu" : "inline",
                        onClick: () => onCreateDraft(item),
                    },
                ]}
            />
        </li>
    )
}

/** 服务端允许动作与当前账号权限共同约束卡片入口。 */
export const visibleActions = (
    item: DefinitionCatalogItem,
    permissions: readonly string[] | undefined,
): DefinitionAllowedAction[] =>
    item.allowed_actions.filter((action) =>
        canPerformCatalogAction(action, item, permissions),
    )
