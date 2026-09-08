"use client"

import Link from "next/link"
import { ArrowUpRight, FileText } from "lucide-react"
import { Button, buttonVariants } from "@/components/ui/button"
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

const BUSINESS_GROUPS: { label: string; types: DocumentType[] }[] = [
    {
        label: "销售相关",
        types: [
            "sales_order",
            "voucher_sales_order",
            "sales_change_order",
            "sales_return_case",
        ],
    },
    {
        label: "采购相关",
        types: [
            "purchase_order",
            "purchase_change_order",
            "purchase_return_order",
        ],
    },
    {
        label: "库存与交付",
        types: [
            "stock_adjustment",
            "purchase_receipt",
            "delivery",
            "electronic_delivery",
            "service_fulfillment",
            "customer_acceptance",
        ],
    },
    {
        label: "收付款与发票",
        types: [
            "customer_receipt",
            "supplier_payment",
            "customer_refund",
            "supplier_refund",
            "receipt_reversal",
            "payment_reversal",
            "invoice",
        ],
    },
]

/** 按业务分区展示审批配置；无需审批的单据使用紧凑条目。 */
export function ProcessCatalog({
    items,
    permissions,
    onCreateDraft,
    onContinueDraft,
    id = "governance-approval-processes-catalog",
}: {
    items: readonly DefinitionCatalogItem[]
    permissions: readonly string[] | undefined
    onCreateDraft: (item: DefinitionCatalogItem) => void
    onContinueDraft: (item: DefinitionCatalogItem) => void
    id?: string
}) {
    return (
        <div className="space-y-8 py-4">
            {BUSINESS_GROUPS.map((group) => {
                const grouped = items.filter((item) =>
                    group.types.includes(item.document_type),
                )
                if (!grouped.length) return null
                const groupId = `${id}-group-${group.types[0]}`
                return (
                    <section key={group.label} aria-labelledby={groupId}>
                        <div className="mb-3 flex items-center gap-3">
                            <h2 id={groupId} className="text-sm font-semibold">
                                {group.label}
                            </h2>
                            <span className="text-xs tabular-nums text-muted-foreground">
                                {grouped.length}
                            </span>
                        </div>
                        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
                            {grouped
                                .filter(
                                    (item) =>
                                        item.approval_requirement !==
                                        "NO_APPROVAL",
                                )
                                .map((item) => {
                                    const actions = visibleActions(
                                        item,
                                        permissions,
                                    )
                                    const segment = toAutomationIdSegment(
                                        item.document_type,
                                    )
                                    const href = `/system/approval-processes/${item.document_type}`
                                    return (
                                        <article
                                            key={item.document_type}
                                            data-document-type={
                                                item.document_type
                                            }
                                            className="flex min-w-0 flex-col rounded-xl border border-border/70 bg-card p-5 transition-colors hover:border-border"
                                        >
                                            <div className="flex items-start justify-between gap-3">
                                                <div className="flex min-w-0 items-center gap-3">
                                                    <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/60 text-muted-foreground">
                                                        <FileText
                                                            className="size-4"
                                                            aria-hidden="true"
                                                        />
                                                    </div>
                                                    <Link
                                                        id={`${id}-row-${segment}-open`}
                                                        href={href}
                                                        className="text-sm font-semibold leading-6 hover:underline focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-4"
                                                    >
                                                        {documentTypeLabel(
                                                            item.document_type,
                                                            item.document_type_label,
                                                        )}
                                                    </Link>
                                                </div>
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
                                            </div>
                                            <div className="mt-5 flex flex-wrap items-end justify-between gap-3 border-t border-border/50 pt-3">
                                                <div className="space-y-1 text-xs text-muted-foreground">
                                                    <p>
                                                        {item.published_version
                                                            ? `当前使用${versionLabel(item.published_version)}`
                                                            : "尚未发布审批流程"}
                                                    </p>
                                                    {item.draft_version ? (
                                                        <p>
                                                            草稿{" "}
                                                            {versionLabel(
                                                                item.draft_version,
                                                            )}{" "}
                                                            · 尚未发布
                                                        </p>
                                                    ) : null}
                                                </div>
                                                <div className="flex flex-wrap items-center gap-1">
                                                    {actions.includes(
                                                        "REPLACE_NODES",
                                                    ) ? (
                                                        <Button
                                                            id={`${id}-row-${segment}-continue`}
                                                            variant="secondary"
                                                            size="sm"
                                                            onClick={() =>
                                                                onContinueDraft(
                                                                    item,
                                                                )
                                                            }
                                                        >
                                                            继续编辑
                                                        </Button>
                                                    ) : (
                                                        <>
                                                            {actions.includes(
                                                                "CREATE_DRAFT",
                                                            ) ? (
                                                                <Button
                                                                    id={`${id}-row-${segment}-create-draft`}
                                                                    variant={
                                                                        item.published_version
                                                                            ? "ghost"
                                                                            : "secondary"
                                                                    }
                                                                    size="sm"
                                                                    onClick={() =>
                                                                        onCreateDraft(
                                                                            item,
                                                                        )
                                                                    }
                                                                >
                                                                    {item.published_version
                                                                        ? "创建新草稿"
                                                                        : "配置流程"}
                                                                </Button>
                                                            ) : null}
                                                            {item.published_version ||
                                                            !actions.includes(
                                                                "CREATE_DRAFT",
                                                            ) ? (
                                                                <Link
                                                                    id={`${id}-row-${segment}-view`}
                                                                    href={href}
                                                                    className={buttonVariants(
                                                                        {
                                                                            variant:
                                                                                "ghost",
                                                                            size: "sm",
                                                                        },
                                                                    )}
                                                                >
                                                                    查看流程
                                                                    <ArrowUpRight
                                                                        className="size-3.5"
                                                                        aria-hidden="true"
                                                                    />
                                                                </Link>
                                                            ) : null}
                                                        </>
                                                    )}
                                                </div>
                                            </div>
                                        </article>
                                    )
                                })}
                        </div>
                        {grouped.some(
                            (item) =>
                                item.approval_requirement === "NO_APPROVAL",
                        ) ? (
                            <div className="mt-3 flex flex-wrap gap-x-6 gap-y-3 rounded-lg bg-muted/30 px-4 py-3">
                                {grouped
                                    .filter(
                                        (item) =>
                                            item.approval_requirement ===
                                            "NO_APPROVAL",
                                    )
                                    .map((item) => (
                                        <div
                                            key={item.document_type}
                                            data-document-type={
                                                item.document_type
                                            }
                                            className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
                                        >
                                            <span>
                                                {documentTypeLabel(
                                                    item.document_type,
                                                    item.document_type_label,
                                                )}
                                            </span>
                                            <span
                                                className="text-muted-foreground"
                                                title="无需配置审批流程"
                                            >
                                                无需审批
                                            </span>
                                        </div>
                                    ))}
                            </div>
                        ) : null}
                    </section>
                )
            })}
        </div>
    )
}

/**
 * 计算目录行可见动作。NO_APPROVAL 永远为空。
 *
 * @param item 目录行
 * @param permissions 已授予权限
 */
export const visibleActions = (
    item: DefinitionCatalogItem,
    permissions: readonly string[] | undefined,
): DefinitionAllowedAction[] =>
    item.allowed_actions.filter((action) =>
        canPerformCatalogAction(action, item, permissions),
    )
