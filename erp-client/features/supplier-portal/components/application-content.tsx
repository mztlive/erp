"use client"
import type { ReactNode } from "react"
import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
} from "@/components/ui/accordion"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { paymentTermLabel } from "@/lib/business-options"
import {
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    RateValue,
} from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import type { NewProductInput, PortalApplication, PortalTerms } from "../types"
import { percentageFromRate } from "../lib/forms"
import { PortalDocument } from "./portal-document"
import { PortalImage } from "./portal-image"
import {
    availabilityLabels,
    statusLabels,
    timeLabel,
} from "../lib/presentation"

function SectionFrame({
    title,
    nested,
    children,
}: {
    title: string
    nested: boolean
    children: ReactNode
}) {
    if (nested) {
        return (
            <section className="space-y-3 border-t border-border/60 pt-4 first:border-t-0 first:pt-0">
                <h4 className="text-body-compact font-semibold text-foreground">
                    {title}
                </h4>
                {children}
            </section>
        )
    }
    return (
        <DetailWorkspaceSection title={title} width="form">
            {children}
        </DetailWorkspaceSection>
    )
}

/** 仅展示业务字段，不把内部身份、任务或对象标识渲染到供应商页面。 */
export function PortalTermsContent({
    terms,
    title = "本次申请条款",
}: {
    terms: PortalTerms
    title?: string
}) {
    return (
        <section className="space-y-3">
            <h3 className="font-semibold">{title}</h3>
            <dl className="grid gap-3 text-body-compact md:grid-cols-2">
                <div>
                    <dt className="text-xs text-muted-foreground">
                        代发含税供货价
                    </dt>
                    <dd>
                        <MoneyValue
                            value={terms.dropship_supply_price_gross}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        集采含税供货价
                    </dt>
                    <dd>
                        <MoneyValue
                            value={terms.bulk_supply_price_gross}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">税率</dt>
                    <dd>
                        <RateValue
                            value={percentageFromRate(terms.input_tax_rate)}
                            precision={2}
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        集采起订量
                    </dt>
                    <dd>
                        {terms.bulk_minimum_order_quantity != null ? (
                            <QuantityValue
                                value={terms.bulk_minimum_order_quantity}
                                unit=""
                            />
                        ) : (
                            "未提供"
                        )}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">有效期</dt>
                    <dd className="num">
                        {terms.valid_from || "未填写"} 起
                        {terms.valid_to
                            ? `，${terms.valid_to} 失效`
                            : "长期有效"}
                    </dd>
                </div>
            </dl>
        </section>
    )
}

export function PortalApplicationContent({
    application,
    showImages = true,
    showHistory = true,
    showDetails = true,
    nested = false,
    assetPrefix = "supplier-portal-current-document",
}: {
    application: PortalApplication
    showImages?: boolean
    showHistory?: boolean
    /** 管理审核页已由对比视图展示明细时可设为 false，仅展示处理与历次提交记录。 */
    showDetails?: boolean
    /** 在折叠面板内部复用时关闭 DetailWorkspaceSection 包裹，避免递归套娃。 */
    nested?: boolean
    assetPrefix?: string
}) {
    const input = ["pending", "effective"].includes(application.status)
        ? (application.submitted_snapshot ?? application.input)
        : application.input
    const terms = input.terms as PortalTerms | undefined
    const product =
        application.kind === "new_product"
            ? (input as unknown as NewProductInput)
            : undefined
    return (
        <>
            {showDetails && (
                <>
                    <SectionFrame title="申请资料" nested={nested}>
                        {application.reason && (
                            <p className="text-body-compact">
                                申请原因：{application.reason}
                            </p>
                        )}
                        {typeof input.supplier_sku_code === "string" && (
                            <p className="text-body-compact">
                                供应商订货编码：
                                <span className="num">
                                    {input.supplier_sku_code}
                                </span>
                            </p>
                        )}
                        {terms && <PortalTermsContent terms={terms} />}
                        {application.kind === "quote" && (
                            <div className="grid gap-2 text-body-compact md:grid-cols-3">
                                <p>
                                    可供情况：
                                    {availabilityLabels[
                                        String(input.availability_status)
                                    ] ?? "未填写"}
                                </p>
                                <p>
                                    可供数量：
                                    {input.available_quantity == null ? (
                                        "未提供"
                                    ) : (
                                        <QuantityValue
                                            value={String(
                                                input.available_quantity,
                                            )}
                                            unit=""
                                        />
                                    )}
                                </p>
                                <p>
                                    实际报送：
                                    <span className="num">
                                        {timeLabel(
                                            input.availability_reported_at as
                                                | number
                                                | undefined,
                                        )}
                                    </span>
                                </p>
                            </div>
                        )}
                        {application.kind === "cooperation" && (
                            <p className="text-body-compact">
                                拟申请付款条件：
                                {typeof input.payment_term === "string"
                                    ? paymentTermLabel(input.payment_term)
                                    : "未填写"}
                            </p>
                        )}
                        {product && showImages && (
                            <div className="flex flex-wrap gap-3">
                                {product.image_asset_ids?.map((id, index) => (
                                    <PortalImage
                                        key={id}
                                        assetId={id}
                                        source={{ request_id: application.id }}
                                        alt={`商品图片 ${index + 1}`}
                                        className="h-28 w-28 rounded-md object-contain"
                                    />
                                ))}
                            </div>
                        )}
                    </SectionFrame>
                    {product &&
                        showImages &&
                        !!product.file_asset_ids?.length && (
                            <SectionFrame title="商品资料附件" nested={nested}>
                                {product.file_asset_ids.map((id, index) => (
                                    <PortalDocument
                                        key={id}
                                        applicationId={application.id}
                                        idPrefix={assetPrefix}
                                        assetId={id}
                                        label={`商品资料 ${index + 1}`}
                                    />
                                ))}
                            </SectionFrame>
                        )}
                    {product && (
                        <SectionFrame
                            title={product.name || "未命名商品"}
                            nested={nested}
                        >
                            <dl className="grid gap-2 text-body-compact md:grid-cols-2">
                                <div>
                                    提报品牌：
                                    {product.brand?.raw_name || "未填写"}
                                </div>
                                <div>
                                    提报分类：
                                    {product.category?.raw_name || "未填写"}
                                </div>
                                <div>型号：{product.model || "未填写"}</div>
                                <div>
                                    说明：{product.description || "未填写"}
                                </div>
                            </dl>
                            {product.skus?.map((sku, index) => (
                                <section
                                    key={sku.row_id}
                                    className="flex flex-col gap-3 border-t border-border/60 pt-3"
                                >
                                    <h3 className="font-medium">
                                        规格{" "}
                                        <span className="num">{index + 1}</span>{" "}
                                        · {sku.name || "未填写名称"}
                                    </h3>
                                    <div className="grid gap-2 text-body-compact md:grid-cols-2">
                                        <p>
                                            规格：
                                            {sku.spec_entries
                                                .map(
                                                    (entry) =>
                                                        `${entry.attribute_code}：${entry.attribute_value_code}`,
                                                )
                                                .join(" / ") || "未填写"}
                                        </p>
                                        <p>提报单位：{sku.unit.raw_name}</p>
                                        <p>
                                            订货编码：
                                            <span className="num">
                                                {sku.ordering_code}
                                            </span>
                                        </p>
                                        <p>
                                            条码：
                                            <span className="num">
                                                {sku.barcode || "未填写"}
                                            </span>
                                        </p>
                                        <p>
                                            可供数量：
                                            {sku.available_quantity == null ? (
                                                "未提供"
                                            ) : (
                                                <QuantityValue
                                                    value={
                                                        sku.available_quantity
                                                    }
                                                    unit={sku.unit.raw_name}
                                                />
                                            )}
                                        </p>
                                        <p>
                                            实际报送：
                                            <span className="num">
                                                {timeLabel(sku.reported_at)}
                                            </span>
                                        </p>
                                    </div>
                                    {showImages && sku.image_asset_id && (
                                        <PortalImage
                                            assetId={sku.image_asset_id}
                                            source={{
                                                request_id: application.id,
                                            }}
                                            alt={sku.name}
                                            className="h-28 w-28 rounded-md object-contain"
                                        />
                                    )}
                                    <PortalTermsContent
                                        terms={sku.supply_terms}
                                        title="首次报价"
                                    />
                                </section>
                            ))}
                        </SectionFrame>
                    )}
                    {product?.skus?.some((sku) => sku.packaging) && (
                        <SectionFrame
                            title="供应商确认的包装及报价依据"
                            nested={nested}
                        >
                            {product.skus
                                .filter((sku) => sku.packaging)
                                .map((sku) => (
                                    <div
                                        key={sku.row_id}
                                        className="space-y-1 text-body-compact"
                                    >
                                        <p>
                                            {sku.name} ·{" "}
                                            {sku.packaging?.original_unit} 转{" "}
                                            {sku.packaging?.base_unit} · 每包装{" "}
                                            <span className="num">
                                                {
                                                    sku.packaging
                                                        ?.units_per_package
                                                }
                                            </span>
                                        </p>
                                        <p>
                                            原包装含税单价：
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    sku.packaging
                                                        ?.original_unit_price
                                                }
                                                taxBasis="gross"
                                            />
                                        </p>
                                        <p>
                                            换算依据：
                                            {sku.quote_basis || "未说明"} ·{" "}
                                            {sku.packaging
                                                ?.conversion_confirmed_by_supplier
                                                ? "供应商已确认基础单位报价及数量"
                                                : "供应商尚未确认换算"}
                                        </p>
                                    </div>
                                ))}
                        </SectionFrame>
                    )}
                </>
            )}
            {application.decisions?.length ? (
                <SectionFrame title="处理记录" nested={nested}>
                    <div className="space-y-2.5">
                        {application.decisions.map((decision, index) => (
                            <div
                                key={index}
                                className="flex flex-wrap items-baseline gap-2 text-body-compact"
                            >
                                <StatusBadge
                                    tone={
                                        decision.decision === "approve"
                                            ? "success"
                                            : "warning"
                                    }
                                    label={
                                        statusLabels[decision.decision] ??
                                        (decision.decision === "approve"
                                            ? "通过"
                                            : "退回")
                                    }
                                />
                                <span>
                                    {decision.comment || "已记录处理结果"}
                                </span>
                                <span className="num text-xs text-muted-foreground">
                                    {timeLabel(decision.at)}
                                </span>
                            </div>
                        ))}
                    </div>
                </SectionFrame>
            ) : null}
            {showHistory && !!application.submissions?.length && (
                <SectionFrame title="历次提交记录" nested={nested}>
                    <Accordion>
                        {application.submissions.map((submission, index) => {
                            const key = String(
                                submission.id ??
                                    submission.submission_no ??
                                    index + 1,
                            )
                            const snapshot =
                                submission.snapshot ?? submission.input ?? {}
                            return (
                                <AccordionItem key={key} value={key}>
                                    <AccordionTrigger
                                        id={`supplier-portal-submission-${toAutomationIdSegment(application.id)}-${toAutomationIdSegment(key)}`}
                                    >
                                        <span className="num">
                                            第{" "}
                                            {submission.submission_no ??
                                                index + 1}{" "}
                                            次提交 ·{" "}
                                            {timeLabel(submission.submitted_at)}
                                        </span>
                                    </AccordionTrigger>
                                    <AccordionContent>
                                        <div className="space-y-4 pt-2">
                                            <PortalApplicationContent
                                                application={{
                                                    ...application,
                                                    status: "pending",
                                                    input: snapshot,
                                                    submitted_snapshot:
                                                        snapshot,
                                                    decisions: [],
                                                    result: undefined,
                                                }}
                                                showImages={showImages}
                                                showHistory={false}
                                                nested
                                                assetPrefix={`supplier-portal-history-document-${toAutomationIdSegment(key)}`}
                                            />
                                        </div>
                                    </AccordionContent>
                                </AccordionItem>
                            )
                        })}
                    </Accordion>
                </SectionFrame>
            )}
            {showDetails && application.result && (
                <SectionFrame title="实际处理结果" nested={nested}>
                    <p className="text-body-compact">
                        {application.result.summary ??
                            (application.kind === "new_product"
                                ? application.result.product_created
                                    ? "审核通过，已建立商品与供货关系；采购方将完成建档与后续合作上架。"
                                    : "审核通过，已完成匹配及供货关系处理；新增规格将由采购方完成后续合作上架，复用规格保持原状态。"
                                : application.result.operation === "REVISE" ||
                                    application.kind === "terms"
                                  ? "已追加供给条款版本。"
                                  : application.kind === "stop"
                                    ? "采购已确认停止供应。"
                                    : application.kind === "cooperation"
                                      ? "付款条件已生效，已生效的历史采购单保持原条件。"
                                      : "已建立供给。")}
                    </p>
                    {application.result.skus?.map((sku, index) => (
                        <p className="text-body-compact" key={sku.row_id}>
                            规格 <span className="num">{index + 1}</span>：
                            {sku.sku_created
                                ? "已入库，待上架"
                                : "复用已有规格，保留原上架状态"}
                        </p>
                    ))}
                </SectionFrame>
            )}
        </>
    )
}
