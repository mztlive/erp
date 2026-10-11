import type { StatusTone } from "@/components/ui/status-badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import type {
    FulfillmentPayment,
    FulfillmentPaymentRequest,
} from "../api/purchase-payment"

type PaymentStatus = { label: string; tone: StatusTone }

export function purchasePaymentStatus(
    payment: FulfillmentPayment,
): PaymentStatus {
    if (compareDecimal(payment.open_amount, "0", 2) <= 0) {
        return compareDecimal(payment.paid_amount, "0", 2) > 0
            ? { label: "已付清", tone: "success" }
            : { label: "无需付款", tone: "neutral" }
    }
    return compareDecimal(payment.paid_amount, "0", 2) > 0
        ? { label: "部分付款", tone: "info" }
        : { label: "未付款", tone: "warning" }
}

/** 合并申请的付款进度以本采购行计算，审批与关闭仍使用申请状态。 */
export function purchaseRequestStatus(
    request: FulfillmentPaymentRequest,
): PaymentStatus {
    switch (request.status) {
        case "draft":
            return { label: "草稿待提交", tone: "neutral" }
        case "in_approval":
            return { label: "审批中", tone: "info" }
        case "rejected":
            return { label: "已驳回", tone: "destructive" }
        case "withdrawn":
            return { label: "已撤回", tone: "neutral" }
        case "closed":
            return { label: "已关闭", tone: "neutral" }
        default:
            if (compareDecimal(request.remaining_amount, "0", 2) <= 0)
                return { label: "本单已付清", tone: "success" }
            return compareDecimal(request.paid_amount, "0", 2) > 0
                ? { label: "本单部分付款", tone: "info" }
                : { label: "已批准待付款", tone: "info" }
    }
}

/** 同时显示进行中的申请状态，避免最新一份申请覆盖其他仍待处理的申请。 */
export function purchaseRequestSummary(
    requests: FulfillmentPaymentRequest[],
): PaymentStatus {
    if (!requests.length) return { label: "未申请", tone: "neutral" }
    const active = requests.filter(
        (request) =>
            request.status === "in_approval" ||
            (["approved", "partially_paid"].includes(request.status) &&
                compareDecimal(request.remaining_amount, "0", 2) > 0),
    )
    const statuses = (active.length ? active : requests.slice(0, 1)).map(
        purchaseRequestStatus,
    )
    return {
        label: [...new Set(statuses.map((status) => status.label))].join("、"),
        tone: statuses[0].tone,
    }
}
