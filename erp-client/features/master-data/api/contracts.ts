import type { Page } from "@/lib/api/paging"
import type { ProductKind } from "@/features/master-data/types"

export type EnableStatus = "active" | "disabled"

export type BackendPage<T> = Page<T>

export type BackendFileAsset = {
  id: string
  storage_object_key: string
  public_url?: string | null
  file_name: string
  content_type: string
  byte_size: number
  security_scan_status: string
  created_by: string
  created_at: number
  version?: number
}


export type ProductCategoryDto = {
  id: string
  category_code: string
  parent_category_id: string | null
  name: string
  product_kind: ProductKind
  status: EnableStatus
  created_at: number
  version: number
}

export type ProductBrandDto = {
  id: string
  brand_code: string
  name: string
  logo_asset_id?: string | null
  status: EnableStatus
  created_at: number
  version: number
}

export type ProductDto = {
  id: string
  product_no: string
  product_kind: ProductKind
  name?: string | null
  category_id?: string | null
  brand_id?: string | null
  status: EnableStatus
  listing_status: "listed" | "partially_listed" | "unlisted"
  listed_sku_count: number
  sku_count: number
  supplied_sku_count?: number
  priced_sku_count?: number
  current_revision_id: string | null
  created_at: number
  version: number
}

export type ProductListingDto = {
  product_id: string
  listing_status: "listed" | "partially_listed" | "unlisted"
  listed_sku_count: number
  sku_count: number
}

export type ProductRevisionDto = {
  id: string
  product_id: string
  revision_no: number
  name: string
  description: string | null
  specification: string | null
  category_id: string
  brand_id: string
  status: EnableStatus
  effective_from: string
  effective_to: string | null
  media?: Array<{
    id: string
    file_asset_id: string
    media_role: string
    sort_order: number
    alt_text?: string | null
  }>
  created_at: number
  version: number
}

export type SkuDto = {
  id: string
  sku_no: string
  product_id: string
  base_unit_id: string
  specification_signature: string
  status: EnableStatus
  listing_status: "listed" | "unlisted"
  current_revision_id: string | null
  created_at: number
  version: number
}

export type SkuRevisionDto = {
  id: string
  sku_id: string
  revision_no: number
  name: string
  description: string | null
  specification: string | null
  barcode: string | null
  source_main_image_asset_id?: string | null
  status: EnableStatus
  sales_visible_price_gross: string | null
  market_price: string | null
  weight_kg: string | null
  volume_m3: string | null
  effective_from: string
  effective_to: string | null
  created_at: number
  version: number
}

export type SellableSkuDto = {
  sku_id: string
  sku_version: number
  sku_revision_id: string
  sku_revision_no: number
  sku_no: string
  product_id: string
  product_no: string
  product_kind: ProductKind
  name: string
  specification_attributes: Array<{
    name: string
    value: string
  }>
  specification: string | null
  barcode: string | null
  base_unit_id: string
  base_unit_code: string | null
  base_unit_name: string | null
  sales_visible_price_gross: string
  market_price: string | null
  main_image_asset_id: string | null
  effective_from: string
  effective_to: string | null
  supplier_count: number
  supply_regions: string[]
  eligibility_as_of: string
}

export type SupplierOfferingSummaryDto = {
  sku_id: string
  supplier_id: string
  status: string
  current_revision_id: string | null
}

export type VoucherCategoryProfileDto = {
  id: string
  sku_id: string
  sku_no?: string | null
  product_id?: string | null
  product_version?: number | null
  name?: string | null
  revision_no: number
  description: string
  status: EnableStatus
  created_at: number
  version: number
}

export type UnitOfMeasureDto = {
  id: string
  unit_code: string
  name: string
  symbol: string
  quantity_scale: number
  status: EnableStatus
  created_at: number
  version: number
}

export type WarehouseDto = {
  id: string
  warehouse_code: string
  status: EnableStatus
  created_at: number
  version: number
}

export type WarehouseRevisionDto = {
  id: string
  warehouse_id: string
  revision_no: number
  name: string
  effective_from: string
  effective_to: string | null
  change_reason: string
  created_at: number
  version: number
}

export type SupplierDto = {
  id: string
  party_id: string
  party_no: string | null
  legal_name: string | null
  short_name: string | null
  party_version: number | null
  supplier_no: string
  default_payment_term_id: string | null
  current_commercial_profile_revision_id: string | null
  status: EnableStatus
  version: number
  created_at: number
  current_profile: CommercialProfileDto | null
}

export type CommercialProfileDto = {
  id: string
  supplier_id: string
  revision_no: number
  settlement_mode: string
  reconciliation_cycle: string
  payment_term_snapshot: string
  invoice_type: string
  invoice_tax_rate: string | null
  signing_entity_party_id: string | null
  signing_entity_name: string | null
  payment_entity_party_id: string | null
  payment_entity_name: string | null
  change_reason: string
  version: number
  created_at: number
}

export type SupplierDetailDto = SupplierDto & {
  party_status: string
  unified_credit_code: string | null
  contacts: PartyContactDto[]
  addresses: PartyAddressDto[]
  tax_profiles: PartyTaxProfileDto[]
  bank_accounts: PartyBankAccountDto[]
  capabilities: SupplierCapabilityDto[]
  qualifications: SupplierQualificationDto[]
  ratings: SupplierRatingDto[]
  commercial_profiles: CommercialProfileDto[]
  sensitive_fields: SupplierSensitiveFieldDto[]
}

export type SupplierSensitiveFieldDto = {
  label: string
  masked_value: string
  reveal_token: string
  expires_at: number
}

export type SupplierCapabilityDto = {
  id: string
  supplier_id: string
  capability_code: string
  service_region: string | null
  owner_user_id: string
  fulfillment_note: string | null
  valid_from: string
  valid_to: string | null
  status: EnableStatus
  version: number
  created_at: number
}

export type SupplierQualificationDto = {
  id: string
  supplier_id: string
  qualification_type: string
  certificate_no: string
  issuer: string | null
  valid_from: string
  valid_to: string | null
  attachment_id: string | null
  status: string
  capability_ids: string[]
  version: number
  created_at: number
}

export type SupplierRatingDto = {
  id: string
  supplier_id: string
  revision_no: number
  initial_score: number | null
  rating: string
  current_score: number
  valid_from: string
  valid_to: string | null
  change_reason: string
  version: number
  created_at: number
}

export type SupplierProfileMutationDto = {
  supplier_id: string
  supplier_no: string
  revision_id: string
  revision_no: number
  supplier_version: number
  effective_from: string
  recorded_at: number
  change_reason: string
}

/** 主体联系人（列表不含 mobile 明文，可用 telephone 回显）。 */
export type PartyContactDto = {
  id: string
  party_id: string
  contact_name: string
  title: string | null
  telephone: string | null
  mobile_masked: string
  email: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

/** 银行账户（列表不含账号明文）。 */
export type PartyBankAccountDto = {
  id: string
  bank_account_no: string
  party_id: string
  account_name: string
  bank_name: string
  account_number_masked: string
  bank_branch_name: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

export type PartyAddressDto = {
  id: string
  party_id: string
  address_type: string
  contact_name: string | null
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

export type PartyTaxProfileDto = {
  id: string
  party_id: string
  tax_no: string
  valid_from: string
  valid_to: string | null
  is_default: boolean
  status: string
  version: number
  created_at: number
}

