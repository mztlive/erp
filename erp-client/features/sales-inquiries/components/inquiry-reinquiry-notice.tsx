import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { inquiryDate } from "../lib/format"
import type { ProcurementInquiry } from "../types"

export function InquiryReinquiryNotice({
    inquiry,
}: {
    inquiry: ProcurementInquiry
}) {
    const request = inquiry.reinquiry
    if (!request) return null
    const products = request.line_ids.map(
        (id) =>
            inquiry.draft.lines.find((line) => line.line_id === id)
                ?.product_name ?? "已调整商品",
    )
    const fees = request.fee_ids.map(
        (id) =>
            [...inquiry.feedbacks]
                .reverse()
                .flatMap((feedback) => feedback.data.fees)
                .find((fee) => fee.fee_id === id)?.name ?? "已调整费用",
    )
    return (
        <Alert variant="info" aria-label="销售再次询价要求">
            <AlertTitle>销售要求再次询价</AlertTitle>
            <AlertDescription>
                <div className="space-y-2">
                    <div className="text-xs">
                        第 {request.previous_feedback_round} 轮反馈后 ·{" "}
                        {inquiryDate(request.at)}
                    </div>
                    <div className="whitespace-pre-wrap wrap-anywhere">
                        <span className="font-medium">原因：</span>
                        {request.reason}
                    </div>
                    {request.concerns && (
                        <div className="whitespace-pre-wrap wrap-anywhere">
                            <span className="font-medium">关注项：</span>
                            {request.concerns}
                        </div>
                    )}
                    <div className="text-xs wrap-anywhere">
                        {request.all_items
                            ? "范围：本任务全部商品及费用"
                            : [
                                  products.length > 0
                                      ? `涉及商品：${products.join("、")}`
                                      : "",
                                  fees.length > 0
                                      ? `涉及费用：${fees.join("、")}`
                                      : "",
                              ]
                                  .filter(Boolean)
                                  .join("；")}
                    </div>
                </div>
            </AlertDescription>
        </Alert>
    )
}
