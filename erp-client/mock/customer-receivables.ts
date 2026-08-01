/**
 * W11 客户往来 seed —— 服务端投影口径。
 * 余额、净分配、逾期状态均在此声明；前端不得重算覆盖。
 */

export type SeedEntry = {
  entryId: string
  entryType: string
  direction: "increase" | "decrease"
  amountGross: string
  dueDate: string
  sourceLabel: string
  postedAt: string
  offsetOfEntryId?: string
}

export type SeedAllocation = {
  allocationId: string
  action: "APPLY" | "REVERSE"
  amountGross: string
  targetLabel: string
  targetId: string
  occurredAt: string
  reverseOfAllocationId?: string
}

export type SeedReceivable = {
  accountId: string
  accountSeq: number
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  salesOrderId: string
  salesOrderNo: string
  businessType: "card" | "physical_service"
  grossTotal: string
  settledTotal: string
  openTotal: string
  invoicedTotal: string
  openInvoiceableTotal: string
  dueDate: string
  dueState: "not_due" | "due_today" | "overdue"
  status: "open" | "partial" | "settled"
  reviewStatus: "na" | "pending_opening" | "reviewed" | "pending_sync_diff"
  baselineVersion: number
  entries: SeedEntry[]
}

export type SeedReceipt = {
  receiptId: string
  receiptNo: string
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  receivedAt: string
  amount: string
  bankReferenceMasked: string
  allocatedTotal: string
  unallocatedAmount: string
  status: "draft" | "posted" | "reversed"
  baselineVersion: number
  allocations: SeedAllocation[]
}

export type SeedInvoice = {
  invoiceId: string
  invoiceCode?: string
  invoiceNo: string
  invoiceKind: "blue" | "red"
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  invoiceDate: string
  grossAmount: string
  netAmount: string
  taxAmount: string
  roundingAdjustmentAmount?: string
  roundingAdjustmentReason?: string
  allocatedTotal: string
  unallocatedAmount: string
  status: "draft" | "registered" | "reversed"
  originalInvoiceId?: string
  baselineVersion: number
  allocations: SeedAllocation[]
}

export const W11_COUNTERPARTIES = [
  {
    counterpartyPartyId: "party_cp_star_hq",
    counterpartyPartyName: "星河制造股份有限公司（总部结算）",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
  },
  {
    counterpartyPartyId: "party_cp_star_branch",
    counterpartyPartyName: "星河制造 · 华南分公司结算户",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
  },
  {
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
  },
  {
    counterpartyPartyId: "party_cp_dongfang",
    counterpartyPartyName: "东方企业服务有限公司",
    customerId: "cust_128",
    customerName: "东方企业服务有限公司",
  },
] as const

/** 指标更新时间：服务端对授权范围投影，不由前端对列表求和 */
export const W11_METRICS_SEED = {
  openReceivableTotal: "520400.00",
  overdueReceivableTotal: "48000.00",
  unallocatedReceiptTotal: "42000.00",
  unallocatedInvoiceTotal: "18600.00",
  cardPendingReviewCount: 1,
} as const

export const W11_RECEIVABLES: readonly SeedReceivable[] = [
  {
    accountId: "ra_star_1001",
    accountSeq: 1,
    counterpartyPartyId: "party_cp_star_hq",
    counterpartyPartyName: "星河制造股份有限公司（总部结算）",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
    salesOrderId: "so_1001",
    salesOrderNo: "XS20260328001",
    businessType: "physical_service",
    grossTotal: "186000.00",
    settledTotal: "0.00",
    openTotal: "186000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "186000.00",
    dueDate: "2026-04-30",
    dueState: "not_due",
    status: "open",
    reviewStatus: "na",
    baselineVersion: 3,
    entries: [
      {
        entryId: "re_star_1001_1",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "186000.00",
        dueDate: "2026-04-30",
        sourceLabel: "销售已生效单 v1 · XS20260328001",
        postedAt: "2026-03-28T10:00:00+08:00",
      },
    ],
  },
  {
    accountId: "ra_star_branch_1",
    accountSeq: 1,
    counterpartyPartyId: "party_cp_star_branch",
    counterpartyPartyName: "星河制造 · 华南分公司结算户",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
    salesOrderId: "so_star_branch_1",
    salesOrderNo: "XS20260410008",
    businessType: "physical_service",
    grossTotal: "52000.00",
    settledTotal: "0.00",
    openTotal: "52000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "52000.00",
    dueDate: "2026-05-15",
    dueState: "not_due",
    status: "open",
    reviewStatus: "na",
    baselineVersion: 1,
    entries: [
      {
        entryId: "re_star_branch_1",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "52000.00",
        dueDate: "2026-05-15",
        sourceLabel: "销售已生效单 v1 · XS20260410008",
        postedAt: "2026-04-10T11:20:00+08:00",
      },
    ],
  },
  {
    accountId: "ra_beichen_1",
    accountSeq: 1,
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
    salesOrderId: "so_beichen_1",
    salesOrderNo: "XS20260215003",
    businessType: "physical_service",
    grossTotal: "136000.00",
    settledTotal: "40000.00",
    openTotal: "96000.00",
    invoicedTotal: "50000.00",
    openInvoiceableTotal: "86000.00",
    dueDate: "2026-03-01",
    dueState: "overdue",
    status: "partial",
    reviewStatus: "na",
    baselineVersion: 5,
    entries: [
      {
        entryId: "re_beichen_1",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "136000.00",
        dueDate: "2026-03-01",
        sourceLabel: "销售已生效单 v1 · XS20260215003",
        postedAt: "2026-02-15T09:00:00+08:00",
      },
    ],
  },
  {
    accountId: "ra_beichen_2",
    accountSeq: 2,
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
    salesOrderId: "so_beichen_2",
    salesOrderNo: "XS20260301012",
    businessType: "physical_service",
    grossTotal: "48000.00",
    settledTotal: "0.00",
    openTotal: "48000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "48000.00",
    dueDate: "2026-03-20",
    dueState: "overdue",
    status: "open",
    reviewStatus: "na",
    baselineVersion: 2,
    entries: [
      {
        entryId: "re_beichen_2",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "48000.00",
        dueDate: "2026-03-20",
        sourceLabel: "销售已生效单 v1 · XS20260301012",
        postedAt: "2026-03-01T14:00:00+08:00",
      },
    ],
  },
  {
    accountId: "ra_dongfang_card",
    accountSeq: 1,
    counterpartyPartyId: "party_cp_dongfang",
    counterpartyPartyName: "东方企业服务有限公司",
    customerId: "cust_128",
    customerName: "东方企业服务有限公司",
    salesOrderId: "so_1013",
    salesOrderNo: "XS20260315020",
    businessType: "card",
    grossTotal: "80000.00",
    settledTotal: "0.00",
    openTotal: "80000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "80000.00",
    dueDate: "2026-08-01",
    dueState: "due_today",
    status: "open",
    reviewStatus: "pending_opening",
    baselineVersion: 1,
    entries: [
      {
        entryId: "re_dongfang_card_1",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "80000.00",
        dueDate: "2026-08-01",
        sourceLabel: "卡券销售已生效单 · XS20260315020",
        postedAt: "2026-03-15T16:00:00+08:00",
      },
    ],
  },
  {
    accountId: "ra_dongfang_2",
    accountSeq: 2,
    counterpartyPartyId: "party_cp_dongfang",
    counterpartyPartyName: "东方企业服务有限公司",
    customerId: "cust_128",
    customerName: "东方企业服务有限公司",
    salesOrderId: "so_dongfang_2",
    salesOrderNo: "XS20260401005",
    businessType: "physical_service",
    grossTotal: "58400.00",
    settledTotal: "0.00",
    openTotal: "58400.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "58400.00",
    dueDate: "2026-05-01",
    dueState: "not_due",
    status: "open",
    reviewStatus: "na",
    baselineVersion: 1,
    entries: [
      {
        entryId: "re_dongfang_2",
        entryType: "SALES_POST",
        direction: "increase",
        amountGross: "58400.00",
        dueDate: "2026-05-01",
        sourceLabel: "销售已生效单 v1 · XS20260401005",
        postedAt: "2026-04-01T10:30:00+08:00",
      },
    ],
  },
]

export const W11_RECEIPTS: readonly SeedReceipt[] = [
  {
    receiptId: "rcpt_beichen_1",
    receiptNo: "SK-20260318-001",
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
    receivedAt: "2026-03-18T11:20:00+08:00",
    amount: "40000.00",
    bankReferenceMasked: "****6281",
    allocatedTotal: "40000.00",
    unallocatedAmount: "0.00",
    status: "posted",
    baselineVersion: 2,
    allocations: [
      {
        allocationId: "rall_beichen_1",
        action: "APPLY",
        amountGross: "40000.00",
        targetLabel: "XS20260215003 · 分录 re_beichen_1",
        targetId: "re_beichen_1",
        occurredAt: "2026-03-18T11:25:00+08:00",
      },
    ],
  },
  {
    receiptId: "rcpt_beichen_unalloc",
    receiptNo: "SK-20260720-014",
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
    receivedAt: "2026-07-20T15:00:00+08:00",
    amount: "42000.00",
    bankReferenceMasked: "****9012",
    allocatedTotal: "0.00",
    unallocatedAmount: "42000.00",
    status: "posted",
    baselineVersion: 1,
    allocations: [],
  },
  {
    receiptId: "rcpt_star_partial",
    receiptNo: "SK-20260701-008",
    counterpartyPartyId: "party_cp_star_hq",
    counterpartyPartyName: "星河制造股份有限公司（总部结算）",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
    receivedAt: "2026-07-01T09:40:00+08:00",
    amount: "50000.00",
    bankReferenceMasked: "****3344",
    allocatedTotal: "0.00",
    unallocatedAmount: "50000.00",
    status: "posted",
    baselineVersion: 1,
    allocations: [],
  },
]

export const W11_INVOICES: readonly SeedInvoice[] = [
  {
    invoiceId: "inv_beichen_blue_1",
    invoiceCode: "044002600111",
    invoiceNo: "25887766",
    invoiceKind: "blue",
    counterpartyPartyId: "party_cp_beichen",
    counterpartyPartyName: "北辰能源集团（结算主体）",
    customerId: "cust_311",
    customerName: "北辰能源集团",
    invoiceDate: "2026-03-25",
    grossAmount: "50000.00",
    netAmount: "44247.79",
    taxAmount: "5752.21",
    allocatedTotal: "50000.00",
    unallocatedAmount: "0.00",
    status: "registered",
    baselineVersion: 2,
    allocations: [
      {
        allocationId: "iall_beichen_1",
        action: "APPLY",
        amountGross: "50000.00",
        targetLabel: "应收子账 ra_beichen_1 · XS20260215003",
        targetId: "ra_beichen_1",
        occurredAt: "2026-03-25T16:00:00+08:00",
      },
    ],
  },
  {
    invoiceId: "inv_star_unalloc",
    invoiceCode: "044002600222",
    invoiceNo: "99112233",
    invoiceKind: "blue",
    counterpartyPartyId: "party_cp_star_hq",
    counterpartyPartyName: "星河制造股份有限公司（总部结算）",
    customerId: "cust_204",
    customerName: "星河制造股份有限公司",
    invoiceDate: "2026-07-10",
    grossAmount: "18600.00",
    netAmount: "16460.18",
    taxAmount: "2139.82",
    allocatedTotal: "0.00",
    unallocatedAmount: "18600.00",
    status: "registered",
    baselineVersion: 1,
    allocations: [],
  },
]

export const W11_DEMO_HAS_DATA_SCOPE = true
