import { FileTextIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"

const warningDetails: Record<string, string> = {
    来源销售缺失: "此次提交未保留可核对的来源销售明细，请联系申请人补充核对依据。",
    供给价未填写: "来源销售未填写预计采购含税单价，无法判断本次采购是否超出预估。",
    数值格式需核对:
        "价格或数量格式异常，无法可靠计算差额，请联系申请人核对原始资料。",
    交期资料不完整: "销售承诺交期或采购预计交期缺失，无法判断能否按时交付。",
    采购单价缺失: "本次提交未记录采购单价，请联系申请人核对计价依据。",
    采购数量缺失: "本次提交未记录采购数量，请联系申请人核对本行采购范围。",
}

/** 仅有新资料或需要解释的差异时提供展开入口。 */
export function hasPurchaseSupplement(
    line: ApprovalPurchaseLine,
    comparison: PurchaseComparison,
    source?: PurchaseSource,
) {
    return Boolean(
        comparison.warnings.length ||
        line.input_tax_rate != null ||
        line.delivery_allocations?.length ||
        source,
    )
}

/** 展开区只展示提交中保存的补充资料及差异解释，不重复主行。 */
export function PurchaseLineSupplement({
    line,
    comparison,
    source,
    prefix,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    source?: PurchaseSource
    prefix: string
    onPreview: (revisionId: string) => void
}) {
    const mismatchedProduct = comparison.warnings.includes("商品 / 规格不一致")
    const warnings = comparison.warnings.filter(
        (warning) => warning !== "商品 / 规格不一致",
    )
    return (
        <div className="grid gap-5 @min-[720px]/document:grid-cols-2">
            {warnings.length > 0 && (
                <section className="space-y-2">
                    <h4 className="text-sm font-medium">异常说明</h4>
                    <ul className="space-y-2 text-sm text-warning-soft-foreground">
                        {warnings.map((warning) => (
                            <li key={warning}>
                                {warning === "高于供给价" &&
                                comparison.priceDifference ? (
                                    <>
                                        每单位高于供给价{" "}
                                        <MoneyValue
                                            value={comparison.priceDifference}
                                            display="exact"
                                        />
                                        {comparison.pricePercent && (
                                            <>
                                                （
                                                <RateValue
                                                    value={
                                                        comparison.pricePercent
                                                    }
                                                    precision={1}
                                                />
                                                ）
                                            </>
                                        )}
                                        {comparison.priceDifferenceTotal && (
                                            <>
                                                ，按本次采购数量计算合计高{" "}
                                                <MoneyValue
                                                    value={
                                                        comparison.priceDifferenceTotal
                                                    }
                                                />
                                            </>
                                        )}
                                        。
                                    </>
                                ) : warning === "本次数量超过销售需求" &&
                                  comparison.quantityExcess ? (
                                    <>
                                        本次采购超过来源行全部销售需求{" "}
                                        <QuantityValue
                                            value={comparison.quantityExcess}
                                            unit={line.unit ?? ""}
                                        />
                                        ，请核对超量采购依据。
                                    </>
                                ) : warning === "单位需核对" ? (
                                    <>
                                        销售单位为“
                                        {line.source?.unit || "未记录"}
                                        ”，采购单位为“{line.unit || "未记录"}
                                        ”；单位不一致，暂不计算单价和数量差额。
                                    </>
                                ) : warning.startsWith("交期晚 ") ? (
                                    <>
                                        预计交付晚于客户承诺{" "}
                                        {comparison.delayDays}{" "}
                                        天，请核对交付安排。
                                    </>
                                ) : (
                                    (warningDetails[warning] ?? warning)
                                )}
                            </li>
                        ))}
                    </ul>
                </section>
            )}
            {mismatchedProduct && line.source && (
                <section className="space-y-2 text-sm">
                    <h4 className="font-medium">商品与规格差异</h4>
                    <dl className="space-y-2 wrap-anywhere">
                        <div>
                            <dt className="text-muted-foreground">销售要求</dt>
                            <dd>
                                {displayName(line.source.title) ||
                                    "未记录商品名称"}{" "}
                                ·{" "}
                                {purchaseSpecification(
                                    line.source.specification,
                                ) || "未记录规格"}
                            </dd>
                        </div>
                        <div>
                            <dt className="text-muted-foreground">本次采购</dt>
                            <dd>
                                {displayName(line.title) || "未记录商品名称"} ·{" "}
                                {purchaseSpecification(line.specification) ||
                                    "未记录规格"}
                            </dd>
                        </div>
                    </dl>
                </section>
            )}
            {line.input_tax_rate != null && (
                <section className="space-y-2 text-sm">
                    <h4 className="font-medium">采购条件</h4>
                    <p className="flex items-baseline gap-3">
                        <span className="text-muted-foreground">进项税率</span>
                        <RateValue
                            value={multiplyFixed(line.input_tax_rate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    </p>
                </section>
            )}
            {source && (
                <section className="space-y-2 text-sm">
                    <h4 className="font-medium">本行来源销售单</h4>
                    <p>
                        {displayBusinessText(
                            source.document_no,
                            source.document_id,
                        ) || "未记录销售单号"}{" "}
                        · 销售版本 V{source.revision_no}
                    </p>
                    <Button
                        id={`${prefix}-source`}
                        variant="link"
                        size="sm"
                        className="h-auto px-0 text-info"
                        onClick={() => onPreview(source.revision_id)}
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览来源销售单
                    </Button>
                </section>
            )}
            {!!line.delivery_allocations?.length && (
                <section className="space-y-2 @min-[720px]/document:col-span-2">
                    <h4 className="text-sm font-medium">交付安排</h4>
                    <ul className="divide-y divide-grid rounded-md border border-grid px-3 text-sm">
                        {line.delivery_allocations.map((delivery) => (
                            <li
                                key={JSON.stringify([
                                    delivery.recipient,
                                    delivery.phone,
                                    delivery.region_code,
                                    delivery.address,
                                ])}
                                className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 py-3"
                            >
                                <div className="min-w-0 flex-1 space-y-1 wrap-anywhere">
                                    <p>
                                        {delivery.recipient}{" "}
                                        <span className="ml-3 num text-muted-foreground">
                                            {delivery.phone}
                                        </span>
                                    </p>
                                    <p className="text-muted-foreground">
                                        {delivery.region_name}
                                        {delivery.address}
                                    </p>
                                </div>
                                <p>
                                    分配数量{" "}
                                    <QuantityValue
                                        value={delivery.quantity}
                                        unit={line.unit ?? ""}
                                    />
                                </p>
                            </li>
                        ))}
                    </ul>
                </section>
            )}
        </div>
    )
}
