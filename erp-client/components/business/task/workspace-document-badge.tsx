import { Badge } from "@/components/ui/badge"
import {
    DOCUMENT_TYPE_CANONICAL_LABELS,
    TASK_TYPE_CANONICAL_LABELS,
} from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"

/** 单据类型徽章色。避免复用 info / success / warning / destructive / rose，防止和状态徽章抢语义。 */
export type WorkspaceDocumentBadgeVariant =
    | "orange"
    | "teal"
    | "violet"
    | "lime"
    | "indigo"
    | "cyan"
    | "neutral"

export type WorkspaceDocumentBadgeMeta = Readonly<{
    label: string
    variant: WorkspaceDocumentBadgeVariant
}>

export type WorkspaceDocumentBadgeItem = Readonly<{
    workItemType: string
    businessObjectType: string
    workItemTypeLabel?: string
    status?: string
}>

type TypeBadgeMeta = Readonly<{
    label: string
    badgeLabel: string
    badgeVariant: WorkspaceDocumentBadgeVariant
}>

type DocumentBadgeMeta = Readonly<{
    approvalLabel: string
    badgeLabel: string
    badgeVariant: WorkspaceDocumentBadgeVariant
}>

const TYPE_BADGE_META: Readonly<Record<string, TypeBadgeMeta>> = {
    APPROVAL_REWORK: {
        label: "驳回待处理",
        badgeLabel: "驳回待处理",
        badgeVariant: "orange",
    },
    SALES_INQUIRY_PROCESSING: {
        label: TASK_TYPE_CANONICAL_LABELS.SALES_INQUIRY_PROCESSING.label,
        badgeLabel:
            TASK_TYPE_CANONICAL_LABELS.SALES_INQUIRY_PROCESSING.shortLabel,
        badgeVariant: "lime",
    },
    SUPPLIER_PORTAL_REVIEW: {
        label: "供应商申请确认",
        badgeLabel: "供应商申请",
        badgeVariant: "lime",
    },
    DOCUMENT_APPROVAL: {
        label: "单据审批",
        badgeLabel: "单据",
        badgeVariant: "indigo",
    },
    PROCUREMENT_ORDER_CREATION: {
        label: TASK_TYPE_CANONICAL_LABELS.PROCUREMENT_ORDER_CREATION.label,
        badgeLabel:
            TASK_TYPE_CANONICAL_LABELS.PROCUREMENT_ORDER_CREATION.shortLabel,
        badgeVariant: "lime",
    },
    PURCHASE_PAYMENT_ARRANGEMENT: {
        label: "采购付款待安排",
        badgeLabel: "付款安排",
        badgeVariant: "lime",
    },
    PURCHASE_SOURCE_UPDATE: {
        label: TASK_TYPE_CANONICAL_LABELS.PURCHASE_SOURCE_UPDATE.label,
        badgeLabel:
            TASK_TYPE_CANONICAL_LABELS.PURCHASE_SOURCE_UPDATE.shortLabel,
        badgeVariant: "orange",
    },
    FULFILLMENT_OPERATION: {
        label: "履约处理",
        badgeLabel: "收发履约",
        badgeVariant: "teal",
    },
    CUSTOMER_ACCEPTANCE_REGISTRATION: {
        label: "客户验收登记",
        badgeLabel: "待客户验收",
        badgeVariant: "teal",
    },
    PURCHASE_RETURN_REFUND: {
        label: "退货款处理",
        badgeLabel: "待处理款项",
        badgeVariant: "cyan",
    },
    PURCHASE_RETURN_INVOICE: {
        label: "退货进项红票",
        badgeLabel: "待红票",
        badgeVariant: "violet",
    },
    SUPPLIER_PAYMENT_EXECUTION: {
        label: "待执行采购付款",
        badgeLabel: "待付款",
        badgeVariant: "cyan",
    },
    SALES_INVOICE_EXECUTION: {
        label: "销项开票处理",
        badgeLabel: "待开票",
        badgeVariant: "violet",
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        label: "供应商结算复核",
        badgeLabel: "供应商结算",
        badgeVariant: "cyan",
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        label: "导入业务确认",
        badgeLabel: "导入确认",
        badgeVariant: "orange",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: TASK_TYPE_CANONICAL_LABELS.INTEGRATION_RESULT_UNKNOWN.label,
        badgeLabel:
            TASK_TYPE_CANONICAL_LABELS.INTEGRATION_RESULT_UNKNOWN.shortLabel,
        badgeVariant: "violet",
    },
    BUSINESS_EXCEPTION: {
        label: "业务异常",
        badgeLabel: "业务异常",
        badgeVariant: "neutral",
    },
}

const DOCUMENT_BADGE_META: Readonly<Record<string, DocumentBadgeMeta>> = {
    sales_order: {
        approvalLabel: "销售单审批",
        badgeLabel: "销售单",
        badgeVariant: "indigo",
    },
    voucher_sales_order: {
        approvalLabel: "卡券销售单审批",
        badgeLabel: "卡券销售",
        badgeVariant: "violet",
    },
    sales_change_order: {
        approvalLabel: "销售变更单审批",
        badgeLabel: "销售变更",
        badgeVariant: "indigo",
    },
    purchase_order: {
        approvalLabel: "采购单审批",
        badgeLabel: "采购单",
        badgeVariant: "orange",
    },
    purchase_change_order: {
        approvalLabel: "采购变更单审批",
        badgeLabel: "采购变更",
        badgeVariant: "lime",
    },
    purchase_payment_request: {
        approvalLabel: "付款申请",
        badgeLabel: "付款申请",
        badgeVariant: "cyan",
    },
    stock_adjustment: {
        approvalLabel: "库存调整单审批",
        badgeLabel: "库存调整",
        badgeVariant: "teal",
    },
    customer_receipt: {
        approvalLabel: "回款复核",
        badgeLabel: "回款",
        badgeVariant: "cyan",
    },
    payable_account: {
        approvalLabel: "供应商付款处理",
        badgeLabel: "待付款",
        badgeVariant: "cyan",
    },
    receivable_account: {
        approvalLabel: "销项开票处理",
        badgeLabel: "待开票",
        badgeVariant: "violet",
    },
    sales_invoice_request: {
        approvalLabel: `${DOCUMENT_TYPE_CANONICAL_LABELS.sales_invoice_request.label}审批`,
        badgeLabel:
            DOCUMENT_TYPE_CANONICAL_LABELS.sales_invoice_request.shortLabel,
        badgeVariant: "violet",
    },
    customer_refund: {
        approvalLabel: "客户退款审批",
        badgeLabel: "客户退款",
        badgeVariant: "cyan",
    },
    supplier_refund: {
        approvalLabel: "供应商退款审批",
        badgeLabel: "供应商退款",
        badgeVariant: "violet",
    },
    receipt_reversal: {
        approvalLabel: "回款冲正审批",
        badgeLabel: "回款冲正",
        badgeVariant: "indigo",
    },
    payment_reversal: {
        approvalLabel: "付款冲正审批",
        badgeLabel: "付款冲正",
        badgeVariant: "lime",
    },
    purchase_receipt: {
        approvalLabel: "采购收货单审批",
        badgeLabel: "采购收货",
        badgeVariant: "orange",
    },
    delivery: {
        approvalLabel: "仓发单审批",
        badgeLabel: "仓发",
        badgeVariant: "teal",
    },
    electronic_delivery: {
        approvalLabel: "电子交付单审批",
        badgeLabel: "电子交付",
        badgeVariant: "cyan",
    },
    service_fulfillment: {
        approvalLabel: "服务履约单审批",
        badgeLabel: "服务履约",
        badgeVariant: "teal",
    },
}

const DOCUMENT_TYPED_WORK_ITEMS = new Set([
    "DOCUMENT_APPROVAL",
    "APPROVAL_INSTANCE",
])

function normalizeObjectType(businessObjectType: string): string {
    return businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}

function documentMetaOf(
    businessObjectType: string,
): DocumentBadgeMeta | undefined {
    const exact = DOCUMENT_BADGE_META[businessObjectType]
    if (exact) return exact
    return DOCUMENT_BADGE_META[normalizeObjectType(businessObjectType)]
}

/**
 * 解析单据/任务类型徽章的短标签与色相；通用审批按单据种类着色，其它任务按任务类型着色。
 */
export function resolveWorkspaceDocumentBadge(
    workItemType: string,
    businessObjectType: string,
    fallbackLabel?: string,
    status = "OPEN",
): WorkspaceDocumentBadgeMeta {
    if (
        DOCUMENT_TYPED_WORK_ITEMS.has(workItemType) ||
        workItemType === "APPROVAL_REWORK"
    ) {
        const documentMeta = documentMetaOf(businessObjectType)
        if (documentMeta) {
            return {
                label: documentMeta.badgeLabel,
                variant: documentMeta.badgeVariant,
            }
        }
    }
    const typeMeta = TYPE_BADGE_META[workItemType]
    if (typeMeta) {
        return {
            label:
                workItemType === "SUPPLIER_PAYMENT_EXECUTION" &&
                normalizeObjectType(businessObjectType) ===
                    "purchase_payment_request" &&
                status !== "OPEN"
                    ? "采购付款"
                    : typeMeta.badgeLabel,
            variant: typeMeta.badgeVariant,
        }
    }
    const label =
        fallbackLabel?.trim() ||
        (DOCUMENT_TYPED_WORK_ITEMS.has(workItemType)
            ? (documentMetaOf(businessObjectType)?.approvalLabel ??
              TYPE_BADGE_META.DOCUMENT_APPROVAL.label)
            : workItemType)
    return { label, variant: "neutral" }
}

export type WorkspaceDocumentBadgeProps = Readonly<
    (
        | {
              item: WorkspaceDocumentBadgeItem
              label?: never
              variant?: never
          }
        | {
              item?: never
              label: string
              variant?: WorkspaceDocumentBadgeVariant
          }
    ) & {
        /** 列表行已有 aria-label 时隐藏给读屏，避免重复朗读类型。 */
        decorative?: boolean
        /** 追加到徽章根节点，例如与单号光学对齐。 */
        className?: string
    }
>

/**
 * 工作台单据类型徽章。按单据或任务类型显示短标签和色相，便于混排队列扫读。
 */
export function WorkspaceDocumentBadge(props: WorkspaceDocumentBadgeProps) {
    const { decorative = false, className } = props
    const badge: WorkspaceDocumentBadgeMeta =
        props.item !== undefined
            ? resolveWorkspaceDocumentBadge(
                  props.item.workItemType,
                  props.item.businessObjectType,
                  props.item.workItemTypeLabel,
                  props.item.status,
              )
            : {
                  label: props.label,
                  variant: props.variant ?? "neutral",
              }

    return (
        <Badge
            variant={badge.variant}
            aria-hidden={decorative ? true : undefined}
            className={cn("shrink-0", className)}
        >
            {badge.label}
        </Badge>
    )
}
