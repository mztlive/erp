"use client"

import { useMutation } from "@tanstack/react-query"
import { DownloadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import {
    downloadInquiryCustomerWorkbook,
    type InquiryCustomerWorkbookProduct,
} from "@/lib/inquiry-workbook"

type InquiryCustomerTemplateButtonProps = {
    /** 已选商品仅用于显示和禁用判断，下载始终重新取得授权后的数据。 */
    products: readonly InquiryCustomerWorkbookProduct[]
    onLoadProducts: () => Promise<readonly InquiryCustomerWorkbookProduct[]>
    /** 仅销售本次明确确认的参考价可以进入客户文件。 */
    confirmedReferencePrices?: Readonly<Record<string, string>>
    inquiry_no?: string
    id?: string
    disabled?: boolean
}

export function InquiryCustomerTemplateButton({
    products,
    onLoadProducts,
    confirmedReferencePrices,
    inquiry_no,
    id = "sales-inquiry-download-customer-template",
    disabled = false,
}: InquiryCustomerTemplateButtonProps) {
    const download = useMutation({
        mutationFn: async () => {
            const currentProducts = await onLoadProducts()
            await downloadInquiryCustomerWorkbook(
                currentProducts.map((product) => ({
                    ...product,
                    sales_reference_price:
                        confirmedReferencePrices?.[product.sku_code] ?? null,
                })),
                inquiry_no,
            )
        },
    })
    return (
        <div className="space-y-2">
            <Button
                id={id}
                type="button"
                variant="outline"
                disabled={disabled || !products.length || download.isPending}
                onClick={() => download.mutate()}
            >
                <DownloadIcon data-icon="inline-start" />
                {download.isPending ? "正在生成…" : "下载客户选品模板"}
            </Button>
            {download.isError ? (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(
                        download.error,
                        "模板下载失败，请重新获取商品后重试",
                    )}
                </p>
            ) : null}
        </div>
    )
}
