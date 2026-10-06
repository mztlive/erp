"use client"
import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
} from "@/components/ui/accordion"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { paymentTermLabel } from "@/lib/business-options"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import type { NewProductInput, PortalApplication, PortalTerms } from "../types"
import { percentageFromRate } from "../lib/forms"
import { PortalDocument } from "./portal-document"
import { PortalImage } from "./portal-image"
import {
    availabilityLabels,
    statusLabels,
    timeLabel,
} from "../lib/presentation"
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
            <dl className="grid gap-3 text-sm md:grid-cols-2">
                <div>
                    <dt className="text-muted-foreground">代发含税供货价</dt>
                    <dd>
                        <MoneyValue
                            value={terms.dropship_supply_price_gross}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">集采含税供货价</dt>
                    <dd>
                        <MoneyValue
                            value={terms.bulk_supply_price_gross}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">税率</dt>
                    <dd>
                        <RateValue
                            value={percentageFromRate(terms.input_tax_rate)}
                            precision={2}
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">集采起订量</dt>
                    <dd>
                        <QuantityValue
                            value={terms.bulk_minimum_order_quantity}
                            unit=""
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">可供区域</dt>
                    <dd>{terms.supply_region?.join("、") || "未填写"}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">有效期</dt>
                    <dd>
                        {terms.valid_from || "未填写"} 至{" "}
                        {terms.valid_to ?? "长期有效"}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">运费</dt>
                    <dd>
                        <MoneyValue
                            value={terms.freight_amount}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">服务费</dt>
                    <dd>
                        <MoneyValue
                            value={terms.service_fee_amount}
                            taxBasis="gross"
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">快递说明</dt>
                    <dd>{terms.dropship_express || "未填写"}</dd>
                </div>
            </dl>
        </section>
    )
}
export function PortalApplicationContent({
    application,
    showImages = true,
    showHistory = true,
    assetPrefix = "supplier-portal-current-document",
}: {
    application: PortalApplication
    showImages?: boolean
    showHistory?: boolean
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
        <div className="space-y-5">
            {application.reason && (
                <p className="text-sm">申请原因：{application.reason}</p>
            )}
            {typeof input.supplier_sku_code === "string" && (
                <p className="text-sm">
                    供应商订货编码：{input.supplier_sku_code}
                </p>
            )}
            {terms && <PortalTermsContent terms={terms} />}
            {application.kind === "quote" && (
                <div className="grid gap-2 text-sm md:grid-cols-3">
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
                                value={String(input.available_quantity)}
                                unit=""
                            />
                        )}
                    </p>
                    <p>
                        实际报送：
                        {timeLabel(
                            input.availability_reported_at as
                                | number
                                | undefined,
                        )}
                    </p>
                </div>
            )}
            {application.kind === "cooperation" && (
                <p className="text-sm">
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
            {product && showImages && !!product.file_asset_ids?.length && (
                <section className="space-y-2">
                    <h2 className="font-semibold">商品资料附件</h2>
                    {product.file_asset_ids.map((id, index) => (
                        <PortalDocument
                            key={id}
                            applicationId={application.id}
                            idPrefix={assetPrefix}
                            assetId={id}
                            label={`商品资料 ${index + 1}`}
                        />
                    ))}
                </section>
            )}
            {product && (
                <section className="space-y-4">
                    <h2 className="font-semibold">
                        {product.name || "未命名商品"}
                    </h2>
                    <dl className="grid gap-2 text-sm md:grid-cols-2">
                        <div>
                            品牌原稿：{product.brand?.raw_name || "未填写"}
                        </div>
                        <div>
                            分类原稿：{product.category?.raw_name || "未填写"}
                        </div>
                        <div>型号：{product.model || "未填写"}</div>
                        <div>说明：{product.description || "未填写"}</div>
                    </dl>
                    {product.skus?.map((sku, index) => (
                        <section
                            key={sku.row_id}
                            className="space-y-3 rounded-lg border p-4"
                        >
                            <h3 className="font-medium">
                                规格 {index + 1} · {sku.name || "未填写名称"}
                            </h3>
                            <div className="grid gap-2 text-sm md:grid-cols-2">
                                <p>
                                    规格：
                                    {sku.spec_entries
                                        .map(
                                            (entry) =>
                                                `${entry.attribute_code}：${entry.attribute_value_code}`,
                                        )
                                        .join(" / ") || "未填写"}
                                </p>
                                <p>单位原稿：{sku.unit.raw_name}</p>
                                <p>订货编码：{sku.ordering_code}</p>
                                <p>条码：{sku.barcode || "未填写"}</p>
                                <p>
                                    可供数量：
                                    {sku.available_quantity == null ? (
                                        "未提供"
                                    ) : (
                                        <QuantityValue
                                            value={sku.available_quantity}
                                            unit={sku.unit.raw_name}
                                        />
                                    )}
                                </p>
                                <p>实际报送：{timeLabel(sku.reported_at)}</p>
                            </div>
                            {showImages && sku.image_asset_id && (
                                <PortalImage
                                    assetId={sku.image_asset_id}
                                    source={{ request_id: application.id }}
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
                </section>
            )}
            {product?.skus?.some((sku) => sku.packaging) && (
                <section className="space-y-2 rounded-lg border p-4">
                    <h2 className="font-semibold">
                        供应商确认的包装及报价依据
                    </h2>
                    {product.skus
                        .filter((sku) => sku.packaging)
                        .map((sku) => (
                            <div key={sku.row_id} className="space-y-1 text-sm">
                                <p>
                                    {sku.name} · {sku.packaging?.original_unit}{" "}
                                    转 {sku.packaging?.base_unit} · 每包装{" "}
                                    {sku.packaging?.units_per_package}
                                </p>
                                <p>
                                    原包装含税单价：
                                    <MoneyValue
                                        value={
                                            sku.packaging?.original_unit_price
                                        }
                                        taxBasis="gross"
                                    />
                                </p>
                                <p>
                                    换算依据：{sku.quote_basis || "未说明"} ·{" "}
                                    {sku.packaging
                                        ?.conversion_confirmed_by_supplier
                                        ? "供应商已确认基础单位报价及数量"
                                        : "供应商尚未确认换算"}
                                </p>
                            </div>
                        ))}
                </section>
            )}
            {application.decisions?.length ? (
                <section className="space-y-2">
                    <h2 className="font-semibold">处理记录</h2>
                    {application.decisions.map((decision, index) => (
                        <p key={index} className="text-sm">
                            {statusLabels[decision.decision] ??
                                (decision.decision === "approve"
                                    ? "通过"
                                    : "退回")}{" "}
                            · {decision.comment || "已记录处理结果"} ·{" "}
                            {timeLabel(decision.at)}
                        </p>
                    ))}
                </section>
            ) : null}
            {showHistory && !!application.submissions?.length && (
                <section className="space-y-3">
                    <h2 className="font-semibold">历次提交原稿</h2>
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
                                        第{" "}
                                        {submission.submission_no ?? index + 1}{" "}
                                        次提交 ·{" "}
                                        {timeLabel(submission.submitted_at)}
                                    </AccordionTrigger>
                                    <AccordionContent>
                                        <PortalApplicationContent
                                            application={{
                                                ...application,
                                                status: "pending",
                                                input: snapshot,
                                                submitted_snapshot: snapshot,
                                                decisions: [],
                                                result: undefined,
                                            }}
                                            showImages={showImages}
                                            showHistory={false}
                                            assetPrefix={`supplier-portal-history-document-${toAutomationIdSegment(key)}`}
                                        />
                                    </AccordionContent>
                                </AccordionItem>
                            )
                        })}
                    </Accordion>
                </section>
            )}
            {application.result && (
                <section className="space-y-2 rounded-lg border bg-muted/30 p-4">
                    <h2 className="font-semibold">实际处理结果</h2>
                    <p className="text-sm">
                        {application.result.summary ??
                            (application.kind === "new_product"
                                ? application.result.product_created
                                    ? "已建立商品及供给；本次新建规格待内部定价并上架。"
                                    : "已完成匹配及供给处理；本次新建规格待内部定价并上架，复用规格保留原上架状态。"
                                : application.result.operation === "REVISE" ||
                                    application.kind === "terms"
                                  ? "已追加供给条款版本。"
                                  : application.kind === "stop"
                                    ? "采购已确认停止供应。"
                                    : application.kind === "cooperation"
                                      ? "付款条件已生效，已冻结采购单保持原条件。"
                                      : "已建立供给。")}
                    </p>
                    {application.result.skus?.map((sku, index) => (
                        <p className="text-sm" key={sku.row_id}>
                            规格 {index + 1}：
                            {sku.sku_created
                                ? "已入库，待上架"
                                : "复用已有规格，保留原上架状态"}
                        </p>
                    ))}
                </section>
            )}
        </div>
    )
}
