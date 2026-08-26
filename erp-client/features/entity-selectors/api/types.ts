export type EntitySelectorPurpose =
    | "filter"
    | "form"
    | "purchase-receipt"
    | "sales-order"
    | "supplier-offering"

export type EntitySearch = Readonly<{
    query: string
    purpose: EntitySelectorPurpose
}>

export type CustomerSearch = EntitySearch &
    Readonly<{
        scope: "mine" | "collaborating" | "assigned" | "all_authorized"
    }>

export type ContractCustomerScope = "assigned"

export type ContractSearch = EntitySearch &
    Readonly<{
        customerId?: string
        selectableOnly?: boolean
        /** 仅当前账号有效归属客户下的合同；缺省不按归属收窄。 */
        scope?: ContractCustomerScope
    }>

export type SellableSkuSearch = EntitySearch &
    Readonly<{ productKind?: string; excludeProductKind?: string }>
