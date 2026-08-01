/**
 * W12 供应商往来种子数据。
 * 正式余额/净分配/门禁一律由 session 投影后返回；此处仅为基线事实。
 */

import type {
  PayableSourceType,
  PayableStatus,
  PaymentStatus,
  InvoiceKind,
  InvoiceStatus,
} from "@/features/supplier-payables/types"

export type SeedPayable = {
  payableAccountId: string
  supplierId: string
  supplierName: string
  sourceType: PayableSourceType
  sourceDocumentId: string
  sourceDocumentNo: string
  primaryEntryId: string
  entryLockVersion: number
  accountLockVersion: number
  grossTotal: string
  /** 净有效付款分配（APPLY-REVERSE） */
  settledTotal: string
  /** 净有效进项票分配 */
  invoicedTotal: string
  dueDate: string
  status: PayableStatus
  paymentGate?: {
    state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
    message: string
    required: string
    allocated: string
    gap: string
  }
}

export type SeedPaymentAlloc = {
  allocationId: string
  action: "APPLY" | "REVERSE"
  payableAccountId: string
  payableEntryId: string
  amount: string
  occurredAt: string
  reverseOfAllocationId?: string
}

export type SeedPayment = {
  paymentId: string
  paymentNo: string
  supplierId: string
  supplierName: string
  paidAt: string
  amount: string
  bankReference: string
  status: PaymentStatus
  allocations: SeedPaymentAlloc[]
  reverseOfPaymentId?: string
  reversedByPaymentId?: string
}

export type SeedInvoiceAlloc = {
  allocationId: string
  action: "APPLY" | "REVERSE"
  payableAccountId: string
  amountGross: string
  occurredAt: string
  reverseOfAllocationId?: string
}

export type SeedInvoice = {
  invoiceId: string
  invoiceCode: string
  invoiceNo: string
  invoiceKind: InvoiceKind
  supplierId: string
  supplierName: string
  invoiceDate: string
  grossAmount: string
  netAmount: string
  taxAmount: string
  status: InvoiceStatus
  originalInvoiceId?: string
  allocations: SeedInvoiceAlloc[]
}

export const W12_SUPPLIERS = [
  { supplierId: "sup_ly", supplierName: "礼遇包装工坊" },
  { supplierId: "sup_hd", supplierName: "华东优选供应链有限公司" },
  { supplierId: "sup_bc", supplierName: "北辰包装" },
  { supplierId: "sup_xc", supplierName: "新程数字科技有限公司" },
  { supplierId: "sup_xg", supplierName: "鲜果直供供应链" },
] as const

/** 默认优先级策略（混合 PO+结算自动分配可用） */
export const W12_DEFAULT_POLICY = {
  payablePriorityPolicyId: "ppp_supplier_default",
  payablePriorityPolicyVersion: 3,
  state: "AVAILABLE" as const,
  mixedAutoAllocationAllowed: true,
}

export const SEED_PAYABLES: SeedPayable[] = [
  {
    payableAccountId: "pa_ly_po02",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentId: "po_02",
    sourceDocumentNo: "CG20260327012",
    primaryEntryId: "pe_ly_po02_1",
    entryLockVersion: 2,
    accountLockVersion: 4,
    grossTotal: "215600.00",
    settledTotal: "50000.00",
    invoicedTotal: "0.00",
    dueDate: "2026-04-02",
    status: "PARTIAL",
    paymentGate: {
      state: "BLOCKED",
      message: "有效已付净核销未达先款 50%",
      required: "107800.00",
      allocated: "50000.00",
      gap: "57800.00",
    },
  },
  {
    payableAccountId: "pa_ly_ss01",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    sourceType: "SUPPLIER_SETTLEMENT",
    sourceDocumentId: "ss_ly_01",
    sourceDocumentNo: "JS20260328001",
    primaryEntryId: "pe_ly_ss01_1",
    entryLockVersion: 1,
    accountLockVersion: 1,
    grossTotal: "42000.00",
    settledTotal: "0.00",
    invoicedTotal: "0.00",
    dueDate: "2026-04-10",
    status: "OPEN",
  },
  {
    payableAccountId: "pa_hd_po04",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentId: "po_04",
    sourceDocumentNo: "CG20260325008",
    primaryEntryId: "pe_hd_po04_1",
    entryLockVersion: 3,
    accountLockVersion: 3,
    grossTotal: "151420.00",
    settledTotal: "0.00",
    invoicedTotal: "50000.00",
    dueDate: "2026-04-14",
    status: "OPEN",
    paymentGate: {
      state: "NOT_APPLICABLE",
      message: "后款条件，无先款门禁",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
  },
  {
    payableAccountId: "pa_bc_po2018",
    supplierId: "sup_bc",
    supplierName: "北辰包装",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentId: "po_2018",
    sourceDocumentNo: "CG20260405001",
    primaryEntryId: "pe_bc_po2018_1",
    entryLockVersion: 1,
    accountLockVersion: 2,
    grossTotal: "28000.00",
    settledTotal: "5000.00",
    invoicedTotal: "0.00",
    dueDate: "2026-04-05",
    status: "PARTIAL",
    paymentGate: {
      state: "BLOCKED",
      message: "先款净核销不足，禁止入库过账",
      required: "28000.00",
      allocated: "5000.00",
      gap: "23000.00",
    },
  },
  {
    payableAccountId: "pa_xc_po05",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentId: "po_05",
    sourceDocumentNo: "CG20260324003",
    primaryEntryId: "pe_xc_po05_1",
    entryLockVersion: 1,
    accountLockVersion: 2,
    grossTotal: "36000.00",
    settledTotal: "36000.00",
    invoicedTotal: "36000.00",
    dueDate: "2026-03-28",
    status: "SETTLED",
    paymentGate: {
      state: "SATISFIED",
      message: "有效付款已满足先款 100%",
      required: "36000.00",
      allocated: "36000.00",
      gap: "0.00",
    },
  },
  {
    payableAccountId: "pa_xg_open",
    supplierId: "sup_xg",
    supplierName: "鲜果直供供应链",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentId: "po_01",
    sourceDocumentNo: "CG20260328001",
    primaryEntryId: "pe_xg_po01_1",
    entryLockVersion: 1,
    accountLockVersion: 1,
    grossTotal: "98000.00",
    settledTotal: "0.00",
    invoicedTotal: "0.00",
    dueDate: "2026-04-05",
    status: "OPEN",
    paymentGate: {
      state: "BLOCKED",
      message: "先款条件未满足，禁止四类履约入口",
      required: "98000.00",
      allocated: "0.00",
      gap: "98000.00",
    },
  },
]

export const SEED_PAYMENTS: SeedPayment[] = [
  {
    paymentId: "pay_ly_01",
    paymentNo: "FK20260327001",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    paidAt: "2026-03-27T15:30:00+08:00",
    amount: "50000.00",
    bankReference: "BANK-LY-8839201",
    status: "POSTED",
    allocations: [
      {
        allocationId: "palloc_ly_01_1",
        action: "APPLY",
        payableAccountId: "pa_ly_po02",
        payableEntryId: "pe_ly_po02_1",
        amount: "50000.00",
        occurredAt: "2026-03-27T15:31:00+08:00",
      },
    ],
  },
  {
    paymentId: "pay_ly_u01",
    paymentNo: "FK20260329002",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    paidAt: "2026-03-29T10:00:00+08:00",
    amount: "25000.00",
    bankReference: "BANK-LY-9910022",
    status: "POSTED",
    allocations: [],
  },
  {
    paymentId: "pay_bc_01",
    paymentNo: "FK20260401003",
    supplierId: "sup_bc",
    supplierName: "北辰包装",
    paidAt: "2026-04-01T09:20:00+08:00",
    amount: "5000.00",
    bankReference: "BANK-BC-110293",
    status: "POSTED",
    allocations: [
      {
        allocationId: "palloc_bc_01_1",
        action: "APPLY",
        payableAccountId: "pa_bc_po2018",
        payableEntryId: "pe_bc_po2018_1",
        amount: "5000.00",
        occurredAt: "2026-04-01T09:21:00+08:00",
      },
    ],
  },
  {
    paymentId: "pay_xc_01",
    paymentNo: "FK20260325004",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    paidAt: "2026-03-25T11:00:00+08:00",
    amount: "36000.00",
    bankReference: "BANK-XC-552001",
    status: "POSTED",
    allocations: [
      {
        allocationId: "palloc_xc_01_1",
        action: "APPLY",
        payableAccountId: "pa_xc_po05",
        payableEntryId: "pe_xc_po05_1",
        amount: "36000.00",
        occurredAt: "2026-03-25T11:01:00+08:00",
      },
    ],
  },
]

export const SEED_INVOICES: SeedInvoice[] = [
  {
    invoiceId: "inv_hd_01",
    invoiceCode: "3100251130",
    invoiceNo: "25883601",
    invoiceKind: "BLUE",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    invoiceDate: "2026-03-28",
    grossAmount: "50000.00",
    netAmount: "44247.79",
    taxAmount: "5752.21",
    status: "POSTED",
    allocations: [
      {
        allocationId: "ialloc_hd_01_1",
        action: "APPLY",
        payableAccountId: "pa_hd_po04",
        amountGross: "50000.00",
        occurredAt: "2026-03-28T14:00:00+08:00",
      },
    ],
  },
  {
    invoiceId: "inv_xc_01",
    invoiceCode: "3100251130",
    invoiceNo: "25884112",
    invoiceKind: "BLUE",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    invoiceDate: "2026-03-26",
    grossAmount: "36000.00",
    netAmount: "33962.26",
    taxAmount: "2037.74",
    status: "POSTED",
    allocations: [
      {
        allocationId: "ialloc_xc_01_1",
        action: "APPLY",
        payableAccountId: "pa_xc_po05",
        amountGross: "36000.00",
        occurredAt: "2026-03-26T10:30:00+08:00",
      },
    ],
  },
  {
    invoiceId: "inv_ly_u01",
    invoiceCode: "3100251144",
    invoiceNo: "25990001",
    invoiceKind: "BLUE",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    invoiceDate: "2026-03-30",
    grossAmount: "18000.00",
    netAmount: "15929.20",
    taxAmount: "2070.80",
    status: "POSTED",
    allocations: [],
  },
]

export function sourceHref(
  sourceType: PayableSourceType,
  sourceDocumentId: string
): string | undefined {
  if (sourceType === "PURCHASE_ORDER") {
    return `/procurement/orders/${sourceDocumentId}`
  }
  return undefined
}

export function maskBankRef(raw: string): string {
  if (raw.length <= 6) return "******"
  return `${raw.slice(0, 4)}****${raw.slice(-3)}`
}
