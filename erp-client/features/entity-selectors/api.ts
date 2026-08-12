import type {
    ContractComboboxItem,
    CustomerComboboxItem,
    ProductComboboxItem,
    SettlementPartyComboboxItem,
    SupplierComboboxItem,
    WarehouseComboboxItem,
} from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

const OPTION_PAGE_SIZE = 30

type SupplierDto = Readonly<{
    id: string
    supplier_no: string
    party_no?: string | null
    legal_name?: string | null
    short_name?: string | null
    status: string
}>

type CustomerDto = Readonly<{
    id: string
    customer_no: string
    legal_name?: string | null
    short_name?: string | null
    party_no?: string | null
    status: string
    owner_user_id?: string | null
    owner_user_name?: string | null
}>

type PartyDto = Readonly<{
    id: string
    party_no: string
    status: string
    current_revision_id?: string | null
}>

type PartyRevisionDto = Readonly<{
    id: string
    legal_name: string
    short_name?: string | null
}>

type WarehouseDto = Readonly<{
    id: string
    warehouse_code: string
    status: string
}>

type WarehouseRevisionDto = Readonly<{
    warehouse_name: string
}>

type SellableSkuDto = Readonly<{
    sku_id: string
    sku_revision_id: string
    sku_no: string
    product_kind: string
    name: string
    specification?: string | null
    base_unit_code?: string | null
    base_unit_name?: string | null
    supplier_count: number
}>

type CompanySkuDto = Readonly<{
    id: string
    sku_no: string
    specification_signature: string
    /** 当前 SKU 修订名称（公司审核后的 SKU 名称）。 */
    name?: string | null
    status: string
}>

type ContractDto = Readonly<{
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    status: string
    current_revision_id?: string | null
}>

type ContractRevisionDto = Readonly<{
    id: string
    revision_no: number
    customer_name: string
    settlement_party_name: string
    valid_to?: string | null
}>

type ContractDetailDto = ContractDto &
    Readonly<{ revisions: readonly ContractRevisionDto[] }>

type SourceSystemDto = Readonly<{
    id: string
    code: string
    name: string
    system_type: string
    status: string
}>

export type EntitySelectorPurpose =
    | "filter"
    | "form"
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

export type ContractSearch = EntitySearch &
    Readonly<{ customerId?: string; selectableOnly?: boolean }>

export type SellableSkuSearch = EntitySearch &
    Readonly<{ productKind?: string; excludeProductKind?: string }>

function activeStatus(status: string) {
    return status.toLowerCase() === "active"
}

function supplierItem(row: SupplierDto): SupplierComboboxItem {
    return {
        supplierId: row.id,
        supplierCode: row.supplier_no,
        supplierName:
            row.legal_name?.trim() ||
            row.short_name?.trim() ||
            row.party_no?.trim() ||
            row.supplier_no,
        statusLabel: activeStatus(row.status) ? "启用" : "停用",
        statusTone: activeStatus(row.status) ? "success" : "neutral",
    }
}

export async function searchSuppliers(
    input: EntitySearch,
): Promise<readonly SupplierComboboxItem[]> {
    const page = await apiGet<Page<SupplierDto>>("/admin/suppliers", {
        keyword: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "supplier_no",
        sort_dir: "asc",
    })
    return page.items.map(supplierItem)
}

export async function fetchSupplierOption(
    supplierId: string,
): Promise<SupplierComboboxItem | null> {
    if (!supplierId) return null
    try {
        return supplierItem(
            await apiGet<SupplierDto>(
                `/admin/suppliers/${encodeURIComponent(supplierId)}`,
            ),
        )
    } catch {
        return null
    }
}

function customerItem(row: CustomerDto): CustomerComboboxItem {
    const enabled = activeStatus(row.status)
    return {
        id: row.id,
        customerNo: row.customer_no,
        legalName:
            row.legal_name?.trim() || row.party_no?.trim() || row.customer_no,
        shortName: row.short_name?.trim() || undefined,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
        ownerName: row.owner_user_name ?? row.owner_user_id ?? undefined,
    }
}

export async function searchCustomers(
    input: CustomerSearch,
): Promise<readonly CustomerComboboxItem[]> {
    const path =
        input.scope === "all_authorized"
            ? "/admin/customers/all-authorized"
            : "/admin/customers"
    const page = await apiGet<Page<CustomerDto>>(path, {
        scope: input.scope,
        keyword: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "updated_at",
        sort_dir: "desc",
    })
    return page.items.map(customerItem)
}

export async function fetchCustomerOption(
    customerId: string,
): Promise<CustomerComboboxItem | null> {
    if (!customerId) return null
    try {
        return customerItem(
            await apiGet<CustomerDto>(
                `/admin/customers/${encodeURIComponent(customerId)}`,
            ),
        )
    } catch {
        return null
    }
}

async function partyItem(row: PartyDto): Promise<SettlementPartyComboboxItem> {
    let displayName = row.party_no
    try {
        const revisions = await apiGet<Page<PartyRevisionDto>>(
            `/admin/parties/${encodeURIComponent(row.id)}/revisions`,
            { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" },
        )
        const revision =
            revisions.items.find(
                (item) => item.id === row.current_revision_id,
            ) ?? revisions.items[0]
        displayName = revision?.legal_name?.trim() || row.party_no
    } catch {
        // 主体仍可按稳定编号选择；名称修订无权限时不伪造名称。
    }
    const enabled = activeStatus(row.status)
    return {
        partyId: row.id,
        partyCode: row.party_no,
        displayName,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}

export async function searchParties(
    input: EntitySearch,
): Promise<readonly SettlementPartyComboboxItem[]> {
    const page = await apiGet<Page<PartyDto>>("/admin/parties", {
        keyword: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "party_no",
        sort_dir: "asc",
    })
    return Promise.all(page.items.map(partyItem))
}

export async function fetchPartyOption(
    partyId: string,
): Promise<SettlementPartyComboboxItem | null> {
    if (!partyId) return null
    try {
        return partyItem(
            await apiGet<PartyDto>(
                `/admin/parties/${encodeURIComponent(partyId)}`,
            ),
        )
    } catch {
        return null
    }
}

async function warehouseItem(
    row: WarehouseDto,
): Promise<WarehouseComboboxItem> {
    let warehouseName = row.warehouse_code
    try {
        const revisions = await apiGet<Page<WarehouseRevisionDto>>(
            "/admin/warehouse-revisions",
            {
                warehouse_id: row.id,
                page: 1,
                page_size: 1,
                sort_by: "revision_no",
                sort_dir: "desc",
            },
        )
        warehouseName =
            revisions.items[0]?.warehouse_name?.trim() || row.warehouse_code
    } catch {
        // 仓库代码是稳定且可展示的回退值。
    }
    const enabled = activeStatus(row.status)
    return {
        warehouseId: row.id,
        warehouseCode: row.warehouse_code,
        warehouseName,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}

export async function searchWarehouses(
    input: EntitySearch,
): Promise<readonly WarehouseComboboxItem[]> {
    const page = await apiGet<Page<WarehouseDto>>("/admin/warehouses", {
        warehouse_code: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "warehouse_code",
        sort_dir: "asc",
    })
    return Promise.all(page.items.map(warehouseItem))
}

export async function fetchWarehouseOption(
    warehouseId: string,
): Promise<WarehouseComboboxItem | null> {
    if (!warehouseId) return null
    const page = await apiGet<Page<WarehouseDto>>("/admin/warehouses", {
        status: "active",
        page: 1,
        page_size: 100,
    })
    const row = page.items.find((item) => item.id === warehouseId)
    return row ? warehouseItem(row) : null
}

function productItem(row: SellableSkuDto): ProductComboboxItem & {
    revisionId: string
} {
    const baseUnit = row.base_unit_name ?? row.base_unit_code ?? undefined
    return {
        productId: row.sku_id,
        revisionId: row.sku_revision_id,
        sku: row.sku_no,
        name: row.name,
        statusLabel: "可销售",
        statusTone: "success",
        baseUnit,
        description: [
            row.specification?.trim(),
            baseUnit ? `单位 ${baseUnit}` : null,
            `有效供应商 ${row.supplier_count}`,
        ]
            .filter(Boolean)
            .join(" · "),
    }
}

export type SellableSkuComboboxItem = ReturnType<typeof productItem>

export async function searchSellableSkus(
    input: SellableSkuSearch,
): Promise<readonly SellableSkuComboboxItem[]> {
    const page = await apiGet<Page<SellableSkuDto>>("/admin/sellable-skus", {
        q: input.query.trim() || undefined,
        product_kind: input.productKind || undefined,
        page: 1,
        page_size: OPTION_PAGE_SIZE,
    })
    return page.items
        .filter(
            (row) =>
                !input.excludeProductKind ||
                row.product_kind.toUpperCase() !==
                    input.excludeProductKind.toUpperCase(),
        )
        .map(productItem)
}

export type CompanySkuComboboxItem = ProductComboboxItem

export async function searchCompanySkus(
    input: EntitySearch,
): Promise<readonly CompanySkuComboboxItem[]> {
    const page = await apiGet<Page<CompanySkuDto>>("/admin/skus", {
        q: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "sku_no",
        sort_dir: "asc",
    })
    return page.items.map((row) => ({
        productId: row.id,
        sku: row.sku_no,
        name: row.name?.trim() || row.specification_signature || row.sku_no,
        statusLabel: "启用",
        statusTone: "success",
        description: row.specification_signature || undefined,
    }))
}

function contractStatus(status: string) {
    switch (status.toUpperCase()) {
        case "EFFECTIVE":
            return { label: "生效中", tone: "success" as const }
        case "TERMINATED":
            return { label: "已终止", tone: "destructive" as const }
        default:
            return { label: "已到期", tone: "neutral" as const }
    }
}

async function contractItem(row: ContractDto): Promise<ContractComboboxItem> {
    let revision: ContractRevisionDto | undefined
    try {
        const detail = await apiGet<ContractDetailDto>(
            `/admin/contracts/${encodeURIComponent(row.id)}`,
        )
        revision =
            detail.revisions.find(
                (item) => item.id === detail.current_revision_id,
            ) ?? detail.revisions[0]
    } catch {
        // 合同稳定编号和状态仍可用于选择；修订摘要按权限降级。
    }
    const status = contractStatus(row.status)
    return {
        contractId: row.id,
        contractNo: row.contract_no,
        customerName: revision?.customer_name ?? row.customer_id,
        statusLabel: status.label,
        statusTone: status.tone,
        revisionNo: revision?.revision_no,
        validTo: revision?.valid_to ?? undefined,
        settlementPartyName: revision?.settlement_party_name,
    }
}

export async function searchContracts(
    input: ContractSearch,
): Promise<readonly ContractComboboxItem[]> {
    const page = await apiGet<Page<ContractDto>>("/admin/contracts", {
        contract_no: input.query.trim() || undefined,
        customer_id: input.customerId || undefined,
        status: input.selectableOnly ? "EFFECTIVE" : undefined,
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return Promise.all(page.items.map(contractItem))
}

export async function fetchContractOption(
    contractId: string,
): Promise<ContractComboboxItem | null> {
    if (!contractId) return null
    try {
        return contractItem(
            await apiGet<ContractDto>(
                `/admin/contracts/${encodeURIComponent(contractId)}`,
            ),
        )
    } catch {
        return null
    }
}

export type MallComboboxItem = Readonly<{
    id: string
    code: string
    name: string
}>

export async function fetchMallOptions(): Promise<readonly MallComboboxItem[]> {
    const page = await apiGet<Page<SourceSystemDto>>("/admin/source-systems", {
        system_type: "MALL",
        status: "active",
        page: 1,
        page_size: 100,
        sort_by: "name",
        sort_dir: "asc",
    })
    return page.items.map((row) => ({
        id: row.id,
        code: row.code,
        name: row.name,
    }))
}
