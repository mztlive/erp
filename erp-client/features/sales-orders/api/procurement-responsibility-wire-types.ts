export type ResolveProcurementResponsibilityLineWire = {
    line_key: string
    sku_id: string
    service_region?: string
}

export type ResolveProcurementResponsibilityRequestWire = {
    lines: ResolveProcurementResponsibilityLineWire[]
}

export type ResolvedProcurementResponsibilityLineWire = {
    line_key: string
    resolved: boolean
    owner_user_id?: string | null
    owner_name?: string | null
    rule_type?: string | null
}

export type ResolveProcurementResponsibilityResponseWire =
    | { lines: ResolvedProcurementResponsibilityLineWire[] }
    | ResolvedProcurementResponsibilityLineWire[]
