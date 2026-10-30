/** 供应商只读采购条件；缺省字段表示未取得来源事实。 */
export type SourcePurchaseTerms = Readonly<{
    minimum_quantity: string | null
    maximum_quantity: string | null
    quantity_multiple: string | null
    restrictions: readonly PurchaseRestriction[]
    quote_context: SourceQuoteContext | null
}>

export type SourceQuoteContext = Readonly<{
    quantity: string
    observed_at: number
    tax_rate: string | null
    tax_rate_raw: string | null
    address_fingerprint: string | null
}>

export type PurchaseRestriction =
    | Readonly<{
          kind:
              | "VIRTUAL_PRODUCT"
              | "INSTALLATION"
              | "APPOINTMENT"
              | "GIFT_SELECTION"
              | "EXTENDED_WARRANTY"
              | "PERSONALIZED_PRICE"
      }>
    | Readonly<{ kind: "OTHER_REQUIRED_PROCESS"; code: string }>
