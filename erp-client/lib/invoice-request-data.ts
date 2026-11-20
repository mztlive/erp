export type InvoiceRequestLine = {
    content: string
    quantity: string
    unit_price: string
}

export type InvoiceRequestData = {
    amount: string
    invoice_title: string
    tax_number: string
    invoice_content: string
    reason: string
    seller_name?: string | null
    seller_tax_number?: string | null
    lines?: InvoiceRequestLine[]
}
